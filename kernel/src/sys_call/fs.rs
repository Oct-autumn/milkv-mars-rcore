use crate::batch::{get_current_user_stack_base, run_next_app};
use crate::config::{APP_BASE_ADDRESS, APP_SIZE_LIMIT};
use crate::{error, print};

const FD_STDOUT: usize = 1;

/// 执行 write 系统调用
///
/// - fd: 文件描述符
/// - buf: 缓冲区指针
/// - len: 缓冲区长度
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> usize {
    // buf不在应用程序区，也不在用户栈区，说明是非法的用户态指针，直接返回0
    if (buf as usize) < get_current_user_stack_base()
        || ((buf as usize + len) > get_current_user_stack_base() + crate::config::USER_STACK_SIZE
            && (buf as usize) < APP_BASE_ADDRESS)
        || (buf as usize + len) > APP_BASE_ADDRESS + APP_SIZE_LIMIT
    {
        error!("sys_write: buf is not in user space: {buf:p}");
        run_next_app();
    }

    match fd {
        FD_STDOUT => {
            let slice = unsafe { core::slice::from_raw_parts(buf, len) };
            let data = core::str::from_utf8(slice).unwrap();
            print!("{}", data);
            len
        }
        _ => {
            error!("Unsupported fd: {fd}");
            0
        }
    }
}
