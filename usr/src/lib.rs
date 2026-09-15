#![no_std]

pub mod console;
mod lang_items;
mod sys_call;

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
        exit(main());
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

/* -------- U-mode 系统调用 -------- */

/// 写入文件
/// - fd: 文件描述符
/// - buf: 要写入的数据
#[allow(unused)]
pub fn write(fd: usize, buf: &[u8]) -> isize {
    sys_call::sys_write(fd, buf)
}

/// 退出程序
/// - code: 退出码
#[allow(unused)]
pub fn exit(code: i32) -> ! {
    sys_call::sys_exit(code)
}

/// 挂起程序
/// - 返回值: isize, 0表示成功
#[allow(unused)]
pub fn yield_() -> isize {
    sys_call::sys_yield()
}
