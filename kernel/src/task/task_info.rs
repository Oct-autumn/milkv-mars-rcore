use super::context::TaskContext;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    /// 还未初始化
    UnInit,
    /// 已初始化，等待调度
    Ready,
    /// 正在运行
    Running,
    /// 已退出
    Exited,
}

#[derive(Clone, Copy)]
pub struct TaskControlBlock {
    pub id: usize,
    pub status: TaskStatus,
    pub cx: TaskContext,
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
