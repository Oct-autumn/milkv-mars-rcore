use super::{SYS_EXIT, SYS_GET_TIME, SYS_YIELD, syscall};

/// 执行 exit 系统调用
///
/// - code: 退出码
#[allow(unused)]
pub fn sys_exit(code: i32) -> ! {
    syscall(SYS_EXIT, [code as usize, 0, 0]);
    unreachable!()
}

/// 执行 yield 系统调用
///
/// - 返回值: isize, 0表示成功
#[allow(unused)]
pub fn sys_yield() -> isize {
    syscall(SYS_YIELD, [0, 0, 0])
}

#[repr(C)]
#[allow(unused)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// 执行 get_time 系统调用
///
/// - ts: 指向 TimeVal 结构的指针，用于存储获取到的时间
/// - _tz: 时区信息，目前未使用
/// - 返回值: isize, 0表示成功，-1表示失败
#[allow(unused)]
pub fn sys_get_time(ts: *mut TimeVal, tz: usize) -> isize {
    syscall(SYS_GET_TIME, [ts as usize, tz, 0])
}
