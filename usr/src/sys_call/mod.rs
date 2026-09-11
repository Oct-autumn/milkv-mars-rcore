mod fs;
mod process;

use core::arch::asm;
pub use fs::sys_write;
pub use process::sys_exit;

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;

/// 执行系统调用
fn syscall(id: usize, args: [usize; 3]) -> isize {
    let mut ret: isize;
    unsafe {
        asm!(
            "ecall",
            inlateout("x10") args[0] => ret,
            in("x11") args[1],
            in("x12") args[2],
            in("x17") id
        );
    }
    ret
}
