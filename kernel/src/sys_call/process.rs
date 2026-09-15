use crate::{
    info,
    task::{exit_current_task_and_run_next, suspend_current_task_and_run_next},
    warn,
};

/// 执行 exit 系统调用
///
/// - code: 退出码
pub fn sys_exit(code: i32) -> ! {
    if code == 0 {
        info!("Program exited with code 0.");
    } else {
        warn!("Program exited with code {}.", code);
    }
    exit_current_task_and_run_next();
}

/// 执行 yield 系统调用
///
/// - 返回值: isize, 0表示成功
pub fn sys_yield() -> isize {
    suspend_current_task_and_run_next();
    0
}
