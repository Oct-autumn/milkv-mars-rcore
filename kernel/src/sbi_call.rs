use sbi::{debug_console, system_reset};

/// 通过 SBI 调用写入调试控制台
pub fn console_putbyte(c: u8) {
    let _ = debug_console::write_byte(c);
    // 实际上，如果写失败，并没有什么能做的。这是目前唯一的读写窗口
}

pub fn shutdown(kind: system_reset::ResetType, reason: system_reset::ResetReason) -> ! {
    let _ = system_reset::system_reset(kind, reason);
    unreachable!()
}
