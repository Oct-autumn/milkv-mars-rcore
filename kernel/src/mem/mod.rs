mod addr;
mod asid;
mod frame;
mod heap_allocator;
mod mem_set;
mod page_table;

use core::arch::asm;

pub use self::asid::{asid_enabled};
pub use addr::{PhysicalAddress, PhysicalPageNumber, VirtualAddress, VirtualPageNumber};
pub use mem_set::{KERNEL_MEM, MemoryMapPermission, MemorySet};
use riscv::register::satp;

pub fn init() {
    heap_allocator::init_heap();
    unsafe {
        // 激活内核内存集
        satp::write(KERNEL_MEM.exclusive_access().satp());
        asm!("sfence.vma");
    }
}
