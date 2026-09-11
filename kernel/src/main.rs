#![no_std]
#![no_main]

mod batch;
mod config;
mod console;
mod lang_items;
mod log;
mod sbi_call;
mod sync;
mod sys_call;
mod time;
mod trap;

use core::arch::global_asm;

global_asm!(include_str!("start.S"));

global_asm! {include_str!(concat!(env!("OUT_DIR"), "/usr_linker.S"))}

#[unsafe(no_mangle)]
fn main(hartid: usize, dtb_addr: usize) -> ! {
    // 接收来自 SBI 传递的参数：
    // a0: hartid, a1: device tree blob (DTB) 的物理地址
    clear_bss(); // 清零 BSS 段

    let _ = config::BASE_ADDRESS;

    // 测试日志输出
    debug!("Debug Console Test: 0x{:x}", 0xdeadbeefu32 as i32);
    info!("Info Console Test: 0x{:x}", 0xdeadbeefu32 as i32);
    warn!("Warn Console Test: 0x{:x}", 0xdeadbeefu32 as i32);
    error!("Error Console Test: 0x{:x}", 0xdeadbeefu32 as i32);

    info!("\n============ Mars-rCore ============");
    info!("BASE_ADDRESS = {:#x}", config::BASE_ADDRESS);
    info!("Boot HartID = {}, DTB Address = {:#x}", hartid, dtb_addr);

    trap::init();

    batch::init();
    batch::run_next_app();
}

#[macro_export]
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
