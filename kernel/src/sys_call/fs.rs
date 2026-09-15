use crate::app_loader::{get_app_base, get_user_stack_base};
use crate::config::{APP_SIZE_LIMIT, USER_STACK_SIZE};
use crate::task::get_current_task;
use crate::{error, print};

const FD_STDOUT: usize = 1;

/// 执行 write 系统调用
///
/// - fd: 文件描述符
/// - buf: 缓冲区指针
/// - len: 缓冲区长度
/// - 返回值: 成功时返回写入的字节数，失败（非法缓冲区/非法 UTF-8/不支持 fd）返回 -1
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let current_task_id = get_current_task();
    let u_stack_base = get_user_stack_base(current_task_id);
    let app_base = get_app_base(current_task_id);
    let buf_start = buf as usize;

    // 校验整个 [buf, buf+len)
    let Some(buf_end) = buf_start.checked_add(len) else {
        error!("sys_write: buffer length overflow: {buf:p}, len={len}");
        return -1;
    };
    let in_user_stack = buf_start >= u_stack_base - USER_STACK_SIZE && buf_end <= u_stack_base;
    let in_app = buf_start >= app_base && buf_end <= app_base + APP_SIZE_LIMIT;
    if !(in_user_stack || in_app) {
        error!("sys_write: buffer out of user space: {buf:p}, len={len}");
        return -1;
    }

    match fd {
        FD_STDOUT => {
            let slice = unsafe { core::slice::from_raw_parts(buf, len) };
            match core::str::from_utf8(slice) {
                Ok(data) => {
                    print!("{}", data);
                    len as isize
                }
                Err(_) => {
                    error!("sys_write: buffer is not valid UTF-8");
                    -1
                }
            }
        }
        _ => {
            error!("Unsupported fd: {fd}");
            -1
        }
    }
}
