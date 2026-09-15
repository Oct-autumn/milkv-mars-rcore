mod context;
mod task_info;

use lazy_static::lazy_static;
use sbi::system_reset::{ResetReason, ResetType};

use crate::{
    app_loader::{get_num_app, init_app_cx},
    config::MAX_APP_NUM,
    debug, info,
    sbi_call::shutdown,
    sync::UPSafeCell,
    task::{
        context::TaskContext,
        task_info::{TaskControlBlock, TaskStatus},
    },
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

    /// 运行第一个任务
    fn run_first_task(&self) -> ! {
        info!("Prepare to run the first task.");
        let mut inner = self.inner.exclusive_access();
        let task0 = &mut inner.tasks[0];
        task0.status = TaskStatus::Running;
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
        debug!("Task {} yielded.", inner.tasks[current].id);
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
            debug!(
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

pub fn run_first_task() -> ! {
    TASK_MANAGER.run_first_task()
}

pub fn suspend_current_task_and_run_next() {
    TASK_MANAGER.suspend_current_task();
}

pub fn exit_current_task_and_run_next() -> ! {
    TASK_MANAGER.exit_current_task()
}
