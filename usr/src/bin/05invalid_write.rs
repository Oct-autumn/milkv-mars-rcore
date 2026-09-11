#![no_std]
#![no_main]

#[macro_use]
extern crate usr_lib;

#[unsafe(no_mangle)]
fn main() -> i32 {
    println!(
        "Into Test sys_write safety check, we will try to write a string from invalid memory..."
    );
    println!("Kernel should refuse the write operation, and return 0.");
    unsafe {
        // 构造一个非法胖指针 &[u8]，指向内核空间的地址 0x40200000
        let invalid_ptr: *const u8 = 0x40200000 as *const u8;
        let invalid_slice: &[u8] = core::slice::from_raw_parts(invalid_ptr, 1);
        usr_lib::write(1, invalid_slice);
    }
    0
}
