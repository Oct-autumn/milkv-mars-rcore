mod context;
mod task_info;

use lazy_static::lazy_static;
use riscv::register::sie;
use sbi::system_reset::{ResetReason, ResetType};

use crate::{
    app_loader::{get_num_app, init_app_cx},
    config::{self, MAX_APP_NUM},
    debug, info,
    sbi_call::shutdown,
    sync::UPSafeCell,
    task::{
        context::TaskContext,
        task_info::{TaskControlBlock, TaskStatus},
    },
    time::{get_time, set_next_timer},
    trace,
};

/// 任务管理器
struct TaskManager {
    inner: UPSafeCell<TaskManagerInner>,
}

/// 任务管理器内部结构体（可变数据）
struct TaskManagerInner {
    current_task: usize,
    tasks: [TaskControlBlock; MAX_APP_NUM],
}

impl TaskManager {
    /// 获取当前任务的 ID
    pub fn get_current_task(&self) -> usize {
        self.inner.exclusive_access().current_task
    }

    /// 计时打点（trap 入口 E2）：结算刚结束的用户态片段，并开启内核态片段。
    ///
    /// trap 只可能来自 U 态，因此自上次打点以来的这段时间即为用户态执行时间。
    fn account_trap_entry(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let now = get_time();
        let tcb = &mut inner.tasks[current];
        tcb.u_run_time += now - tcb.last_enter_time;
        tcb.last_enter_time = now;
    }

    /// 计时打点（trap 出口 E3）：结算刚结束的内核态片段，并开启用户态片段。
    fn account_trap_exit(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let now = get_time();
        let tcb = &mut inner.tasks[current];
        tcb.k_run_time += now - tcb.last_enter_time;
        tcb.last_enter_time = now;
    }

    /// 读取当前任务的 (用户态运行时间, 内核态运行时间)，单位：微秒。
    ///
    /// 调用时当前内核态片段尚未闭合（要等返回用户态或被换出才结算），
    /// 故用 `now - last_enter_time` 将其补足。
    fn current_task_times(&self) -> (usize, usize) {
        let inner = self.inner.exclusive_access();
        let tcb = &inner.tasks[inner.current_task];
        let now = get_time();
        (tcb.u_run_time, tcb.k_run_time + (now - tcb.last_enter_time))
    }

    fn init(&self) {
        info!("Prepare to run the first task.");
        unsafe {
            sie::set_stimer(); // 使能S-mode时钟中断
        }
        set_next_timer(config::STIMER_INTERVAL);
    }

    /// 运行第一个任务
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let task0 = &mut inner.tasks[0];
        task0.status = TaskStatus::Running;
        // 计时打点（换入 E1）：首个用户态片段从当前时刻开始
        task0.last_enter_time = get_time();
        debug!("Task {} starts running.", task0.id);
        let next_task_cx_ptr = &task0.cx as *const TaskContext;
        drop(inner);

        let mut _unused = TaskContext::zero_init();
        unsafe {
            context::__switch(&mut _unused as *mut TaskContext, next_task_cx_ptr);
        }
        unreachable!("Should not return to run_first_task");
    }

    /// 查找下一个可运行任务（ready 状态）
    ///
    /// 从当前任务的下一项开始环形遍历**全部**任务（含当前任务自身）：
    /// 这样当“只剩当前任务自己 Ready”时（例如它刚 yield、其他任务都已退出），
    /// 仍会返回当前任务，而不会被误判成“无任务可运行”。
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let n = inner.tasks.len();
        let current = inner.current_task;
        (1..=n)
            .map(|offset| (current + offset) % n)
            .find(|&idx| inner.tasks[idx].status == TaskStatus::Ready)
    }

    /// 挂起当前任务
    pub fn suspend_current_task(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task; // 由于Rust的借用规则，必须先获取current_task的值
        inner.tasks[current].status = TaskStatus::Ready;
        trace!("Task {} yielded.", inner.tasks[current].id);
        drop(inner); // 释放锁，避免在切换上下文时发生死锁

        self.run_next_task();
    }

    /// 退出当前任务
    pub fn exit_current_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task; // 由于Rust的借用规则，必须先获取current_task的值
        inner.tasks[current].status = TaskStatus::Exited;
        debug!("Task {} exited.", inner.tasks[current].id);
        drop(inner); // 释放锁，避免在切换上下文时发生死锁

        self.run_next_task();

        unreachable!("Should not return to exit_current_task, as the current task has exited.");
    }

    /// 运行下一个任务
    fn run_next_task(&self) {
        // 查找下一个可运行的任务
        if let Some(next_task) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            if next_task == current {
                // 仅当前任务自身可运行（它刚 yield 且其他任务都已退出）：
                // 无需切换，把状态复位为 Running 后直接返回，让当前任务继续执行。
                inner.tasks[current].status = TaskStatus::Running;
                return;
            }
            inner.tasks[next_task].status = TaskStatus::Running;
            inner.current_task = next_task;
            // 计时打点（换出 E4 / 换入 E1）：__switch 只在内核中被调用，故被换下的
            // 任务此刻必处于内核态，把本段内核耗时结算进 k_run_time；被换入的任务
            // 从当前时刻开启新片段，从而把 off-CPU 的时间排除在统计之外。
            let now = get_time();
            let current_tcb = &mut inner.tasks[current];
            current_tcb.k_run_time += now - current_tcb.last_enter_time;
            inner.tasks[next_task].last_enter_time = now;
            trace!(
                "Task switch: {} -> {}",
                inner.tasks[current].id, inner.tasks[next_task].id
            );

            let current_task_cx_ptr = &mut inner.tasks[current].cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next_task].cx as *const TaskContext;

            drop(inner); // 释放锁，避免在切换上下文时发生死锁

            unsafe {
                context::__switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // 这里我们不添加unreachable!宏，因为如果当前任务是被挂起的，
            // 那么下一次该任务被调度时__switch函数会在从这里继续执行。
        } else {
            // 没有更多可运行的任务，内核执行关机；
            info!("No more ready tasks to run! Kernel will shutdown.");
            shutdown(ResetType::Shutdown, ResetReason::NoReason);
        }
    }
}

lazy_static! {
    static ref TASK_MANAGER: TaskManager = {
        let num_app = get_num_app();
        let mut tasks = [TaskControlBlock {
            id: 0,
            status: TaskStatus::UnInit,
            cx: TaskContext::zero_init(),
            u_run_time: 0,
            k_run_time: 0,
            last_enter_time: 0,
        }; MAX_APP_NUM];
        for (i, tcb) in tasks.iter_mut().enumerate().take(num_app) {
            tcb.id = i;
            tcb.cx = TaskContext::goto_restore(init_app_cx(i));
            tcb.status = TaskStatus::Ready;
        }
        TaskManager {
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                })
            },
        }
    };
}

pub fn get_current_task() -> usize {
    TASK_MANAGER.get_current_task()
}

pub fn account_trap_entry() {
    TASK_MANAGER.account_trap_entry();
}

pub fn account_trap_exit() {
    TASK_MANAGER.account_trap_exit();
}

pub fn current_task_times() -> (usize, usize) {
    TASK_MANAGER.current_task_times()
}

pub fn run_first_task() -> ! {
    TASK_MANAGER.init();
    TASK_MANAGER.run_first_task()
}

pub fn suspend_current_task_and_run_next() {
    TASK_MANAGER.suspend_current_task();
}

pub fn exit_current_task_and_run_next() -> ! {
    TASK_MANAGER.exit_current_task()
}
