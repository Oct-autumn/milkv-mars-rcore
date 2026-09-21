use alloc::vec::Vec;
use lazy_static::lazy_static;

use crate::{
    config::{MEM_END_ADDR, PAGE_SIZE_SHIFT},
    mem::addr::PhysicalPageNumber,
    sync::UPSafeCell,
};

const PAGE_SIZE: usize = 1 << PAGE_SIZE_SHIFT;

trait FrameAllocator {
    /// 创建一个新的物理页分配器
    fn new() -> Self;
    /// 分配一个物理页，返回其物理页号
    fn alloc(&mut self) -> Option<PhysicalPageNumber>;
    /// 释放一个物理页
    fn dealloc(&mut self, ppn: PhysicalPageNumber);
}

pub struct StackFrameAllocator {
    /// 当前可分配的物理页号范围：[current, end)
    current: usize,
    end: usize,

    /// 已回收的物理页号列表
    recycled: Vec<usize>,
}

impl FrameAllocator for StackFrameAllocator {
    /// 创建一个新的物理页分配器
    fn new() -> Self {
        unsafe extern "C" {
            safe fn ekernel();
        }
        let ekernel_ptr = ekernel as *const () as usize;
        Self {
            // current 指向内核结束地址的下一个页号
            current: (ekernel_ptr + PAGE_SIZE - 1) >> PAGE_SIZE_SHIFT,
            // end 指向内存结束地址的页号
            end: MEM_END_ADDR >> PAGE_SIZE_SHIFT,
            recycled: Vec::new(),
        }
    }

    fn alloc(&mut self) -> Option<PhysicalPageNumber> {
        if let Some(ppn) = self.recycled.pop() {
            Some(ppn.into())
        } else {
            if self.current == self.end {
                None
            } else {
                self.current += 1;
                Some((self.current - 1).into())
            }
        }
    }

    fn dealloc(&mut self, ppn: PhysicalPageNumber) {
        let ppn = ppn.into();
        // validity check
        if ppn >= self.current || self.recycled.iter().find(|&v| *v == ppn).is_some() {
            panic!("Frame ppn={:#x} has not been allocated!", ppn);
        }
        // recycle
        self.recycled.push(ppn);
    }
}

lazy_static! {
    static ref FRAME_ALLOCATOR: UPSafeCell<StackFrameAllocator> =
        unsafe { UPSafeCell::new(StackFrameAllocator::new()) };
}

pub struct FrameTracker {
    ppn: PhysicalPageNumber,
}

impl FrameTracker {
    /// 创建一个新的 FrameTracker
    ///
    /// 这意味着一个新的页被分配，FrameTracker将负责跟踪该页的生命周期。
    /// 创建时，FrameTracker会初始化该页的内容为零。
    pub fn new(ppn: PhysicalPageNumber) -> Self {
        ppn.as_raw_page().fill(0);
        Self { ppn }
    }

    /// 获取该 FrameTracker 对应的物理页号
    ///
    /// 设计该方法的目的是为了在需要时获取物理页号，而不是直接暴露内部字段。
    pub fn ppn(&self) -> PhysicalPageNumber {
        self.ppn
    }
}

impl Drop for FrameTracker {
    /// 当 FrameTracker 被销毁时，释放其占用的物理页
    fn drop(&mut self) {
        FRAME_ALLOCATOR.exclusive_access().dealloc(self.ppn);
    }
}

/// 分配一个物理页，返回其 FrameTracker
///
/// 说明：当前内核没有虚拟内存机制，物理内存耗尽直接等于内存耗尽 \
/// 后续加入虚拟内存机制（swap）可处理物理内存耗尽的情况（LRU等）
pub fn alloc_frame() -> Option<FrameTracker> {
    FRAME_ALLOCATOR
        .exclusive_access()
        .alloc()
        .map(FrameTracker::new)
}
