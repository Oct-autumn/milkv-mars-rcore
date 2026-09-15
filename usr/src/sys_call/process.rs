use super::{SYS_EXIT, SYS_YIELD, syscall};

/// 执行 exit 系统调用
///
/// - code: 退出码
pub fn sys_exit(code: i32) -> ! {
    syscall(SYS_EXIT, [code as usize, 0, 0]);
    unreachable!()
}

/// 执行 yield 系统调用
///
/// - 返回值: isize, 0表示成功
pub fn sys_yield() -> isize {
    syscall(SYS_YIELD, [0, 0, 0])
}
