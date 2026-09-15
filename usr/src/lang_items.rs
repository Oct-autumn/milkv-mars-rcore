use core::panic::PanicInfo;

use crate::{println, sys_call::process::sys_exit};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("panic: {}", info.message());
    sys_exit(-1);
}
