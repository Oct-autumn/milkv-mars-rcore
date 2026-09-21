use alloc::vec::Vec;
use riscv::register::satp::{self, Satp};

use crate::mem::{addr::VirtualPageNumber, frame::alloc_frame};

use super::{addr::PhysicalPageNumber, frame::FrameTracker};

bitflags! {
    pub struct PTEFlags: u8 {
        const V = 1 << 0; // Valid
        const R = 1 << 1; // Read
        const W = 1 << 2; // Write
        const X = 1 << 3; // Execute
        const U = 1 << 4; // User
        const G = 1 << 5; // Global
        const A = 1 << 6; // Accessed
        const D = 1 << 7; // Dirty
    }
}

/// 页表项
///
/// 由物理页号和页表项标志拼接组成
#[derive(Copy, Clone)]
#[repr(C)]
pub struct PageTableEntry(usize);

impl PageTableEntry {
    const PPN_SHIFT: usize = 10;

    pub fn new(ppn: PhysicalPageNumber, flags: PTEFlags) -> Self {
        Self(usize::from(ppn) << PageTableEntry::PPN_SHIFT | flags.bits() as usize)
    }

    pub fn empty() -> Self {
        Self(0)
    }

    pub fn ppn(&self) -> PhysicalPageNumber {
        PhysicalPageNumber::from(self.0 >> PageTableEntry::PPN_SHIFT)
    }

    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits_truncate(self.0 as u8)
    }

    pub fn is_valid(&self) -> bool {
        self.flags().contains(PTEFlags::V)
    }
}

impl From<PageTableEntry> for usize {
    fn from(pte: PageTableEntry) -> Self {
        pte.0
    }
}

/// 页表
pub struct PageTable {
    /// 根页表对应的物理页号
    root_ppn: PhysicalPageNumber,
    /// 页表管理的物理页列表（用于绑定生命周期，在 drop 时释放）
    frames: Vec<FrameTracker>,
}

impl PageTable {
    pub fn new() -> Self {
        let root_frame =
            alloc_frame().unwrap_or_else(|| panic!("Out of Memory: Failed to alloc new frame!"));
        Self {
            root_ppn: root_frame.ppn(),
            frames: vec![root_frame], // 此处将根页表对应的物理页加入 frames，以便在 PageTable 被 drop 时释放
        }
    }

    /// 查找虚拟页号 vpn 对应的页表项
    fn get_pte(&self, vpn: VirtualPageNumber) -> Option<&mut PageTableEntry> {
        let idxs = vpn.index();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, &idx) in idxs.iter().enumerate() {
            let pte = &mut ppn.as_page_table()[idx];
            if i == 2 {
                // 找到对应的页表项，返回 Some(&mut PageTableEntry)
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                // 中途遇到无效的页表项，说明该虚拟页号未映射，返回 None
                return None;
            }
            ppn = pte.ppn();
        }
        result
    }

    /// 查找或创建虚拟页号 vpn 对应的页表项
    fn get_or_create_pte(&mut self, vpn: VirtualPageNumber) -> &mut PageTableEntry {
        let idxs = vpn.index();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, &idx) in idxs.iter().enumerate() {
            let pte = &mut ppn.as_page_table()[idx];
            if i == 2 {
                // 找到对应的页表项，返回 Some(&mut PageTableEntry)
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                // 中途遇到无效的页表项，说明该虚拟页号未映射，需要创建新的页表项
                let frame = alloc_frame()
                    .unwrap_or_else(|| panic!("Out of Memory: Failed to alloc new frame!"));
                *pte = PageTableEntry::new(frame.ppn(), PTEFlags::V);
                self.frames.push(frame);
            }
            ppn = pte.ppn();
        }
        result.unwrap()
    }

    pub fn map(&mut self, vpn: VirtualPageNumber, ppn: PhysicalPageNumber, flags: PTEFlags) {
        let pte = self.get_or_create_pte(vpn);
        if pte.is_valid() {
            panic!(
                "Failed to map: virtual page number {:#x} is already mapped!",
                usize::from(vpn)
            );
        }
        // 叶子 PTE 需预先置位 A/D：U74 采用 trap-based A/D 管理，若叶子 PTE 未置
        // A/D，首次访问会触发 page fault（QEMU 会自动置位，故仅在实机暴露）。
        // 非叶子 PTE 在 get_or_create_pte 中以 V-only 创建，不受本处影响。
        *pte = PageTableEntry::new(ppn, flags | PTEFlags::V | PTEFlags::A | PTEFlags::D);
    }

    pub fn unmap(&mut self, vpn: VirtualPageNumber) {
        let pte = self.get_pte(vpn).unwrap_or_else(|| {
            panic!(
                "Failed to unmap: virtual page number {:#x} is not mapped!",
                usize::from(vpn)
            )
        });
        *pte = PageTableEntry::empty();
    }

    pub fn to_satp(&self) -> Satp {
        let satp =
            Satp::from_bits((satp::Mode::Sv39.into_usize() << 60) | usize::from(self.root_ppn)); // ASID 暂时置空
        satp
    }

    /* 用于手动MMU的辅助方法 */

    /// 翻译VPN为对应的PTE
    pub fn translate_pte(&self, vpn: VirtualPageNumber) -> Option<PageTableEntry> {
        self.get_pte(vpn)
            .filter(|pte| pte.is_valid())
            .map(|pte| *pte)
    }

    /// 翻译VPN为PPN
    pub fn translate(
        &self,
        vpn: VirtualPageNumber,
        perm_check: Option<PTEFlags>,
    ) -> Option<PhysicalPageNumber> {
        self.translate_pte(vpn)
            .filter(|pte| perm_check.is_none_or(|perm| pte.flags().contains(perm)))
            .map(|pte| pte.ppn())
    }
}
