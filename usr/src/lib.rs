#![no_std]

pub mod console;
mod lang_items;
mod sys_call;

pub use sys_call::{fs, process};

macro_rules! linker_symbol_addr {
    ($symbol:path) => {
        ($symbol as *const ()).addr()
    };
}

/* -------- U-mode 程序 -------- */

unsafe extern "Rust" {
    fn main() -> i32;
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
pub extern "C" fn _start() -> ! {
    clear_bss();
    unsafe {
        process::sys_exit(main());
    }
}

fn clear_bss() {
    unsafe extern "C" {
        safe fn start_bss();
        safe fn end_bss();
    }
    (linker_symbol_addr!(start_bss)..linker_symbol_addr!(end_bss)).for_each(|addr| unsafe {
        (addr as *mut u8).write_volatile(0);
    });
}
