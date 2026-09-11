use super::{SYS_WRITE, syscall};

/// 执行 write 系统调用
///
/// - fd: 文件描述符
/// - buf: 缓冲区胖指针（包含了指针和长度）
pub fn sys_write(fd: usize, buf: &[u8]) -> isize {
    syscall(SYS_WRITE, [fd, buf.as_ptr() as usize, buf.len()])
}
