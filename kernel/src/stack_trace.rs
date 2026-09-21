use core::arch::asm;

use crate::error;

const MAX_DEPTH: usize = 64;

/// 打印当前栈的调用链
/// 我们在 .cargo/config.toml 中启用了 `-C force-frame-pointers=yes`，
/// 因此每个函数都会在栈上保存调用者的帧指针（s0/fp），
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn print_stack_trace() {
    let mut fp: usize;
    asm!("mv {}, s0", out(reg) fp);

    error!("==== Stack trace ====");
    for _ in 0..MAX_DEPTH {
        if fp == 0 {
            break;
        }
        let saved_ra = *((fp as *const usize).sub(1));
        let saved_fp = *((fp as *const usize).sub(2));

        error!("  0x{:016x} -> 0x{:016x}", saved_ra, saved_fp);

        // 栈向低地址增长，调用者的帧指针必然更大；否则说明帧指针链已回绕。
        if saved_fp <= fp {
            break;
        }
        fp = saved_fp;
    }
    error!("=====================");
}
