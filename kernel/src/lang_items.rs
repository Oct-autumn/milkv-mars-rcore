use sbi::system_reset::{ResetReason, ResetType};

use crate::{error, sbi_call::shutdown, stack_trace::print_stack_trace};
use core::panic::PanicInfo;

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
    shutdown(ResetType::Shutdown, ResetReason::SystemFailure);
}
