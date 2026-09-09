#![no_std]
#![no_main]

mod config;
mod console;
mod lang_items;
mod sbi_call;

use core::arch::global_asm;

use sbi::system_reset::{ResetReason, ResetType};

global_asm!(include_str!("start.S"));

#[unsafe(no_mangle)]
fn main(hartid: usize, dtb_addr: usize) -> ! {
    // 接收来自 SBI 传递的参数：
    // a0: hartid, a1: device tree blob (DTB) 的物理地址
    clear_bss(); // 清零 BSS 段

    let _ = config::BASE_ADDRESS;

    print!("\n");
    print!("============ Mars-rCore ============\n");
    print!("BASE_ADDRESS = {:#x}\n", config::BASE_ADDRESS);
    print!("Boot HartID = {}, DTB Address = {:#x}\n", hartid, dtb_addr);

    sbi_call::shutdown(ResetType::ColdReboot, ResetReason::NoReason);
}

macro_rules! linker_symbol_addr {
    ($symbol:path) => {
        ($symbol as *const ()).addr()
    };
}

fn clear_bss() {
    unsafe extern "C" {
        safe fn sbss();
        safe fn ebss();
    }
    (linker_symbol_addr!(sbss)..linker_symbol_addr!(ebss))
        .for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}
