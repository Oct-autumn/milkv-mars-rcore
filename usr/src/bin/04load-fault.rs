#![no_std]
#![no_main]

use core::ptr::{null_mut, read_volatile};

#[macro_use]
extern crate usr_lib;

#[unsafe(no_mangle)]
fn main() -> i32 {
    println!("\nload_fault APP running...\n");
    println!("Into Test load_fault, we will insert an invalid load operation...");
    println!("Kernel should kill this application!");
    unsafe {
        let _i = read_volatile(null_mut::<u8>());
    }
    0
}
