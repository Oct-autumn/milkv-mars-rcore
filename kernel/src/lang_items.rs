use crate::{error, stack_trace::print_stack_trace};
use core::{arch::asm, panic::PanicInfo};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("Kernel panicked: {}", info.message());
    if let Some(location) = info.location() {
        error!(
            "  --> {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    }
    unsafe {
        print_stack_trace();
        asm!("wfi");
    }
    // 由于WFI指令只是建议式的，因此我们在这里使用一个无限循环来确保CPU不会继续执行其他指令。
    loop {}
}
