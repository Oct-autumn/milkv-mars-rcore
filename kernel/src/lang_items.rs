use core::{arch::asm, panic::PanicInfo};

use crate::{error, stack_trace::print_stack_trace};

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
    }
    // 该平台经 SBI shutdown 实际表现为重启，会冲刷串口现场；
    // 故 panic 后直接 wfi 挂起，便于完整保留并读取日志。
    loop {
        unsafe { asm!("wfi") };
    }
}
