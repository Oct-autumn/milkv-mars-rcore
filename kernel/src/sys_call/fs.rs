use crate::print;

const FD_STDOUT: usize = 1;

/// 执行 write 系统调用
///
/// - fd: 文件描述符
/// - buf: 缓冲区指针
/// - len: 缓冲区长度
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> usize {
    match fd {
        FD_STDOUT => {
            let slice = unsafe { core::slice::from_raw_parts(buf, len) };
            let data = core::str::from_utf8(slice).unwrap();
            print!("{}", data);
            len
        }
        _ => {
            panic!("Unsupported fd: {fd}");
        }
    }
}
