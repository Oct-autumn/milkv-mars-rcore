use super::{SYS_EXIT, syscall};

/// 执行 exit 系统调用
///
/// - code: 退出码
pub fn sys_exit(code: i32) -> ! {
    syscall(SYS_EXIT, [code as usize, 0, 0]);
    unreachable!()
}
