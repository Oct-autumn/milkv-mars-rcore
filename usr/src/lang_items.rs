use core::panic::PanicInfo;

use crate::{exit, println};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("panic: {}", info.message());
    exit(-1);
}
