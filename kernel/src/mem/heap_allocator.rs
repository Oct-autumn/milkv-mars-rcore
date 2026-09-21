// os/src/mm/heap_allocator.rs

use crate::{
    config::{K_HEAP_SIZE, K_HEAP_SIZE_SHIFT},
    info, linker_symbol_addr,
};
use buddy_system_allocator::LockedHeap;

const K_HEAP_ORDER: usize = K_HEAP_SIZE_SHIFT + 1;

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<K_HEAP_ORDER> = LockedHeap::empty();

static mut HEAP_SPACE: [u8; K_HEAP_SIZE] = [0; K_HEAP_SIZE];

/// 初始化内核堆
pub fn init_heap() {
    unsafe {
        HEAP_ALLOCATOR
            .lock()
            .init(&raw mut HEAP_SPACE as usize, K_HEAP_SIZE);
        // 本项目使用的高版本Rust编译器不允许使用 HEAP_SPACE.as_ptr() 获取地址。
        // 由于 HEAP_SPACE 是一个静态变量，它的地址在编译时就已经确定，因此可以直接使用
        // &raw mut HEAP_SPACE as usize 来获取它的地址。
        // 这是一种可行的替代方法，确保在初始化堆时使用正确的内存地址。
    }
}

#[alloc_error_handler]
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}

/// 内核堆动态内存分配测试
#[allow(unused)]
pub fn heat_test() {
    use alloc::boxed::Box;
    use alloc::vec::Vec;
    unsafe extern "C" {
        safe fn sbss();
        safe fn ebss();
    }
    let bss_range = linker_symbol_addr!(sbss)..linker_symbol_addr!(ebss);
    let a = Box::new(5);
    assert_eq!(*a, 5);
    assert!(bss_range.contains(&(a.as_ref() as *const _ as usize)));
    drop(a);
    let mut v: Vec<usize> = Vec::new();
    for i in 0..500 {
        v.push(i);
    }
    for (i, &v_it) in v.iter().enumerate().take(500) {
        assert_eq!(v_it, i);
    }
    assert!(bss_range.contains(&(v.as_ptr() as usize)));
    drop(v);
    info!("heap_test passed!");
}
