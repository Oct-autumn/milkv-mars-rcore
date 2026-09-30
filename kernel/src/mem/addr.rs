use core::ops::Add;

use crate::config::{K_V_MEM_OFFSET, PAGE_SIZE, PAGE_SIZE_SHIFT};

use super::page_table::PageTableEntry;

/// 物理地址宽度
const PA_WIDTH_SV39: usize = 56;
/// 物理页号宽度
const PPN_WIDTH_SV39: usize = PA_WIDTH_SV39 - PAGE_SIZE_SHIFT;
/// 物理地址掩码
const PA_MASK_SV39: usize = (1 << PA_WIDTH_SV39) - 1;
/// 页内偏移掩码
const PAGE_OFFSET_MASK: usize = PAGE_SIZE - 1;
/// 页表索引位宽
const PT_INDEX_WIDTH_SV39: usize = 9;
/// 页表索引掩码
const PT_INDEX_MASK_SV39: usize = (1 << PT_INDEX_WIDTH_SV39) - 1;

/// 物理地址
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysicalAddress(usize);

impl From<usize> for PhysicalAddress {
    fn from(addr: usize) -> Self {
        Self(addr & PA_MASK_SV39)
    }
}

impl From<PhysicalPageNumber> for PhysicalAddress {
    fn from(ppn: PhysicalPageNumber) -> Self {
        Self(ppn.0 << PAGE_SIZE_SHIFT)
    }
}

impl From<PhysicalAddress> for usize {
    fn from(pa: PhysicalAddress) -> Self {
        pa.0
    }
}

impl Add for PhysicalAddress {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl PhysicalAddress {
    /// 获取页内偏移
    pub fn page_offset(&self) -> usize {
        self.0 & PAGE_OFFSET_MASK
    }

    /// 向上取整到页对齐
    pub fn ceil_page(&self) -> PhysicalPageNumber {
        PhysicalPageNumber((self.0 + PAGE_OFFSET_MASK) >> PAGE_SIZE_SHIFT)
    }

    /// 向下取整到页对齐
    pub fn floor_page(&self) -> PhysicalPageNumber {
        PhysicalPageNumber(self.0 >> PAGE_SIZE_SHIFT)
    }
}

/// 物理页号
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysicalPageNumber(usize);

impl From<usize> for PhysicalPageNumber {
    fn from(ppn: usize) -> Self {
        Self(ppn & ((1 << PPN_WIDTH_SV39) - 1))
    }
}

impl From<PhysicalPageNumber> for usize {
    fn from(ppn: PhysicalPageNumber) -> Self {
        ppn.0
    }
}

impl From<PhysicalAddress> for PhysicalPageNumber {
    fn from(pa: PhysicalAddress) -> Self {
        assert_eq!(
            pa.0 & PAGE_OFFSET_MASK,
            0,
            "PA is not page aligned, please use PhysicalAddress::floor_page() or PhysicalAddress::ceil_page() to get a page aligned PPN."
        );
        Self(pa.0 >> PAGE_SIZE_SHIFT)
    }
}

impl Add for PhysicalPageNumber {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl PhysicalPageNumber {
    /// 访问原始页
    pub fn as_raw_page(&self) -> &'static mut [u8] {
        let pa: usize = self.0 << PAGE_SIZE_SHIFT;
        let va: usize = pa + K_V_MEM_OFFSET;    // 将物理地址映射到内核虚拟地址
        unsafe { core::slice::from_raw_parts_mut(va as *mut u8, PAGE_SIZE) }
    }

    /// 以页表形式访问原始页
    pub fn as_page_table(&self) -> &'static mut [PageTableEntry] {
        let pa: usize = self.0 << PAGE_SIZE_SHIFT;
        let va: usize = pa + K_V_MEM_OFFSET;    // 将物理地址映射到内核虚拟地址
        unsafe {
            core::slice::from_raw_parts_mut(
                va as *mut PageTableEntry,
                PAGE_SIZE / size_of::<PageTableEntry>(),
            )
        }
    }

    /// 以 mut T 形式访问原始页
    pub fn as_mut_type<T>(&self) -> &'static mut T {
        let pa: usize = self.0 << PAGE_SIZE_SHIFT;
        let va: usize = pa + K_V_MEM_OFFSET;    // 将物理地址映射到内核虚拟地址
        unsafe { &mut *(va as *mut T) }
    }
}

/// 虚拟地址
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtualAddress(usize);

impl From<usize> for VirtualAddress {
    fn from(addr: usize) -> Self {
        Self(addr)
    }
}

impl From<VirtualPageNumber> for VirtualAddress {
    fn from(vpn: VirtualPageNumber) -> Self {
        Self(vpn.0 << PAGE_SIZE_SHIFT)
    }
}

impl From<VirtualAddress> for usize {
    fn from(va: VirtualAddress) -> Self {
        va.0
    }
}

impl Add for VirtualAddress {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl VirtualAddress {
    /// 获取页内偏移
    pub fn page_offset(&self) -> usize {
        self.0 & PAGE_OFFSET_MASK
    }

    /// 向上取整到页对齐
    pub fn ceil_page(&self) -> VirtualPageNumber {
        VirtualPageNumber((self.0 + PAGE_OFFSET_MASK) >> PAGE_SIZE_SHIFT)
    }

    /// 向下取整到页对齐
    pub fn floor_page(&self) -> VirtualPageNumber {
        VirtualPageNumber(self.0 >> PAGE_SIZE_SHIFT)
    }
}

/// 虚拟页号
///
/// Sv39采用三级页表管理，39位虚拟地址。
/// 针对4KB页大小，页内偏移占12位，剩余27位用于虚拟页号。
/// 27位虚拟页号被分为3段，每段9位，分别对应三级页表的索引。
///
/// 对于大页，Sv39支持两类大页：（本Kernel尚未实现支持）
/// - 2MB大页：两级页表，页内偏移占21位，剩余18位用于虚拟页号；
/// - 1GB大页：一级页表，页内偏移占30位，剩余9位用于虚拟页号。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtualPageNumber(usize);

impl From<usize> for VirtualPageNumber {
    fn from(vpn: usize) -> Self {
        Self(vpn)
    }
}

impl From<VirtualPageNumber> for usize {
    fn from(value: VirtualPageNumber) -> Self {
        value.0
    }
}

impl From<VirtualAddress> for VirtualPageNumber {
    fn from(va: VirtualAddress) -> Self {
        assert_eq!(
            va.0 & PAGE_OFFSET_MASK,
            0,
            "VA is not page aligned, please use VirtualAddress::floor_page() or VirtualAddress::ceil_page() to get a page aligned VPN."
        );
        Self(va.0 >> PAGE_SIZE_SHIFT)
    }
}

impl Add for VirtualPageNumber {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl VirtualPageNumber {
    /// 计算各级索引
    ///
    /// Sv39 页表自顶向下依次使用 VPN[2]、VPN[1]、VPN[0]，
    /// 因此 idx[0] 必须是最高的 9 位（根页表索引），需从高位开始切分。
    pub fn index(&self) -> [usize; 3] {
        let mut vpn = self.0;
        let mut idx = [0; 3];
        for i in (0..3).rev() {
            idx[i] = vpn & PT_INDEX_MASK_SV39;
            vpn >>= PT_INDEX_WIDTH_SV39;
        }
        idx
    }
}
