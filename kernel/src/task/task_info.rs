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
}
