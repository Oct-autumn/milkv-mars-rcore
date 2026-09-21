#![no_std]
#![no_main]

use core::ptr::null_mut;

#[macro_use]
extern crate usr_lib;

#[unsafe(no_mangle)]
fn main() -> i32 {
    println!("\nstore_fault APP running...\n");
    println!("Into Test store_fault, we will insert an invalid store operation...");
    println!("Kernel should kill this application!");
    unsafe {
        null_mut::<u8>().write_volatile(1);
    }
    0
}
