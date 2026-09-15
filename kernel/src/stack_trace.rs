use core::arch::asm;

use crate::error;

/// 打印当前栈的调用链
/// 我们在 .cargo/config.toml 中启用了 `-C force-frame-pointers=yes`，
/// 因此每个函数都会在栈上保存调用者的帧指针（s0/fp），
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn print_stack_trace() {
    let mut fp: *const usize;
    asm!("mv {}, s0", out(reg) fp);

    error!("==== Stack trace ====");
    while !fp.is_null() {
        let saved_ra = *fp.sub(1);
        let saved_fp = *fp.sub(2);

        error!("  0x{:016x} -> 0x{:016x}", saved_ra, saved_fp);

        fp = saved_fp as *const usize;
    }
    error!("=====================");
}
