use riscv::register::satp::Satp;

use crate::{
    config::{K_STACK_SIZE, K_STACK_V_BASE, PAGE_SIZE, TRAP_CONTEXT},
    linker_symbol_addr,
    mem::{KERNEL_MEM, MemoryMapPermission, MemorySet, PhysicalPageNumber, VirtualAddress},
    trap::{TrapContext, trap_handler},
};

use super::context::TaskContext;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    /// 已初始化，等待调度
    Ready,
    /// 正在运行
    Running,
    /// 已退出
    Exited,
}

pub struct TaskControlBlock {
    /// 任务的唯一标识符
    pub id: usize,
    /// 任务的状态
    pub status: TaskStatus,
    /// 任务的上下文信息
    pub cx: TaskContext,
    /// 任务的内存集
    pub memory_set: MemorySet,
    /// 陷阱上下文所在的物理页号
    pub trap_cx_ppn: PhysicalPageNumber,
    /// 应用数据大小
    #[allow(unused)]
    pub base_size: usize,

    /// 用户态累计运行时间（单位：微秒）
    pub u_run_time: usize,
    /// 内核态累计运行时间（单位：微秒）
    pub k_run_time: usize,
    /// 本任务最近一次“开始占用 CPU”的时刻（单位：微秒）
    ///
    /// 用于把“在 CPU 上”的执行按特权级切成 u/k 片段：
    /// - 陷入内核态时，把 now - last_enter_time 记入 u_run_time；
    /// - 返回用户态、或被换下 CPU 时，把 now - last_enter_time 记入 k_run_time。
    ///
    /// 该字段在任务管理器锁的保护下读写。
    pub last_enter_time: usize,
}

impl TaskControlBlock {
    /// 获取任务的Trap上下文引用
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.as_mut_type()
    }

    /// 获取任务的satp
    pub fn get_satp(&self) -> Satp {
        self.memory_set.get_satp()
    }

    pub fn new(elf_data: &[u8], app_id: usize) -> Self {
        // 解析 ELF 文件，创建内存集
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data)
            .unwrap_or_else(|e| panic!("invalid app ELF (app_id={app_id}): {e}"));

        // 计算陷阱上下文在用户空间的物理页号
        let trap_cx_ppn = memory_set
            .translate(VirtualAddress::from(TRAP_CONTEXT), None)
            .unwrap()
            .into();

        // 在内核空间为该任务分配内核栈，并映射到内核虚拟地址空间
        let (kernel_stack_bottom, kernel_stack_top) = kernel_stack_position(app_id);
        KERNEL_MEM.exclusive_access().insert_framed_area(
            kernel_stack_bottom.into(),
            kernel_stack_top.into(),
            MemoryMapPermission::R | MemoryMapPermission::W,
            None,
        );

        // 创建任务控制块
        let task_control_block = Self {
            id: app_id,
            status: TaskStatus::Ready,
            cx: TaskContext::goto_trap_return(kernel_stack_top),
            memory_set,
            trap_cx_ppn,
            base_size: user_sp,
            u_run_time: 0,
            k_run_time: 0,
            last_enter_time: 0,
        };

        // 初始化任务的 TrapContext
        let trap_cx = task_control_block.get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_MEM.exclusive_access().get_satp().bits(),
            kernel_stack_top,
            linker_symbol_addr!(trap_handler),
        );
        task_control_block
    }
}

pub fn kernel_stack_position(app_id: usize) -> (usize, usize) {
    let kernel_stack_top = K_STACK_V_BASE - app_id * (K_STACK_SIZE + PAGE_SIZE);
    let kernel_stack_bottom = kernel_stack_top - K_STACK_SIZE;
    (kernel_stack_bottom, kernel_stack_top)
}
