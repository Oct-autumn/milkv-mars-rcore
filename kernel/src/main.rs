#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

mod app_loader;
mod config;
mod console;
mod lang_items;
mod log;
mod mem;
mod sbi_call;
mod stack_trace;
mod sync;
mod sys_call;
mod task;
mod time;
mod trap;
mod utils;

#[macro_use]
extern crate alloc;
#[macro_use]
extern crate bitflags;

use core::arch::global_asm;

use riscv::register::sstatus;

global_asm!(concat!(
    include_str!(concat!(env!("OUT_DIR"), "/kernel_consts.S")),
    include_str!("start.S"),
));

global_asm! {include_str!(concat!(env!("OUT_DIR"), "/usr_linker.S"))}

#[unsafe(no_mangle)]
fn main(hartid: usize, dtb_addr: usize) -> ! {
    // 接收来自 SBI 传递的参数：
    // a0: hartid, a1: device tree blob (DTB) 的物理地址
    clear_bss(); // 清零 BSS 段

    // 打印当前生效的日志等级（用 println! 而非受过滤的 log 宏，
    // 便于区分“该等级没有日志触发”与“日志被等级过滤”）
    println!("Mars-rCore log level: {}", config::LOG_LEVEL_NAME);

    // 初始化内核堆
    mem::init();

    info!("\n============ Mars-rCore ============");
    info!("BASE_ADDRESS = {:#x}", config::K_BASE_ADDRESS);
    info!("Boot HartID = {}, DTB Address = {:#x}", hartid, dtb_addr);

    task::run_first_task();
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
    // 显式启用硬浮点单元
    unsafe {
        sstatus::set_fs(sstatus::FS::Initial);
    }

    (linker_symbol_addr!(sbss)..linker_symbol_addr!(ebss))
        .for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}
