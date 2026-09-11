use crate::error;
use core::{arch::asm, panic::PanicInfo};

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    error!("Kernel Panic: {}", _info.message());
    unsafe {
        asm!("wfi");
    }
    // 由于WFI指令只是建议式的，因此我们在这里使用一个无限循环来确保CPU不会继续执行其他指令。
    loop {}
}
