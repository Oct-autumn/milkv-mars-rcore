use core::cmp::min;

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use lazy_static::lazy_static;
use riscv::register::satp::Satp;
use xmas_elf::ElfFile;

use crate::{
    config::{MEM_END_ADDR, PAGE_SIZE, PAGE_SIZE_SHIFT, TRAMPOLINE, TRAP_CONTEXT, U_STACK_SIZE},
    info, linker_symbol_addr,
    mem::{
        PhysicalAddress,
        addr::{PhysicalPageNumber, VirtualAddress, VirtualPageNumber},
        frame::{FrameTracker, alloc_frame},
        page_table::{PTEFlags, PageTable},
    },
    sync::UPSafeCell,
    trap,
    utils::Range,
};

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MemoryMapType {
    /// 直接映射
    Identical,
    /// 间接映射
    Framed,
}

bitflags! {
    pub struct MemoryMapPermission: u8 {
        const R = 1 << 1; // Read
        const W = 1 << 2; // Write
        const X = 1 << 3; // Execute
        const U = 1 << 4; // User
    }
}

impl From<PTEFlags> for MemoryMapPermission {
    fn from(flags: PTEFlags) -> Self {
        Self::from_bits(flags.bits() & MemoryMapPermission::all().bits()).unwrap()
    }
}

impl From<MemoryMapPermission> for PTEFlags {
    fn from(perm: MemoryMapPermission) -> Self {
        Self::from_bits(perm.bits()).unwrap()
    }
}

/// 映射区段（逻辑段）
///
/// 一段连续的虚拟内存，其中的每一页都以相同的方式映射到物理页帧，具有相同的权限
pub struct MemoryMapArea {
    vpn_range: Range<VirtualPageNumber>,
    data_frames: BTreeMap<VirtualPageNumber, FrameTracker>,
    map_type: MemoryMapType,
    map_perm: MemoryMapPermission,
}

impl MemoryMapArea {
    /// 创建一个新的映射区段
    ///
    /// 注意：刚创建出来的区段只是一个逻辑段，还没有映射到页表中，只有调用了 `map` 方法后才会真正映射到页表中
    ///
    /// ## 参数
    /// - `start_va` 起始虚拟地址（包含）
    /// - `end_va` 结束虚拟地址（不包含）
    /// - `map_type` 映射类型
    /// - `map_perm` 映射权限
    pub fn new(
        start_va: VirtualAddress,
        end_va: VirtualAddress,
        map_type: MemoryMapType,
        map_perm: MemoryMapPermission,
    ) -> Self {
        Self {
            vpn_range: Range::new(start_va.floor_page(), end_va.ceil_page(), |vpn| {
                vpn + VirtualPageNumber::from(1)
            }),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }

    /// 将一个虚拟页映射到页表中
    fn map_one(&mut self, pt: &mut PageTable, vpn: VirtualPageNumber) {
        let ppn = match self.map_type {
            MemoryMapType::Identical => {
                // 直接映射，虚拟页号等于物理页号
                PhysicalPageNumber::from(usize::from(vpn))
            }
            MemoryMapType::Framed => {
                // 间接映射，需要分配物理页帧
                let frame = alloc_frame()
                    .unwrap_or_else(|| panic!("Out of Memory: Failed to alloc new frame!"));
                let ppn: PhysicalPageNumber = frame.ppn();
                self.data_frames.insert(vpn, frame);
                ppn
            }
        };
        // 此处的unwarp是安全的
        // 因为 MemoryMapPermission 与 PTEFlags 的低位位布局相同，所以可以通过bits直接转换
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits()).unwrap();
        pt.map(vpn, ppn, pte_flags);
    }

    /// 将一个虚拟页从页表中解除映射
    fn unmap_one(&mut self, pt: &mut PageTable, vpn: VirtualPageNumber) {
        match self.map_type {
            MemoryMapType::Identical => {
                // 直接映射，不需要释放物理页帧
            }
            MemoryMapType::Framed => {
                // 间接映射，需要释放物理页帧
                self.data_frames.remove(&vpn);
            }
        }
        pt.unmap(vpn);
    }

    /// 将整个映射区段映射到页表中
    pub fn map(&mut self, pt: &mut PageTable) {
        for vpn in self.vpn_range {
            self.map_one(pt, vpn);
        }
    }

    /// 将整个映射区段从页表中解除映射
    pub fn unmap(&mut self, pt: &mut PageTable) {
        for vpn in self.vpn_range {
            self.unmap_one(pt, vpn);
        }
    }

    /// 将数据写入到映射区段中
    ///
    /// 仅用于初始化映射区段的数据，**不应**用于运行时的数据写入
    pub fn copy_data_from(&mut self, pt: &mut PageTable, data: &[u8]) {
        assert_eq!(self.map_type, MemoryMapType::Framed);

        // 校验数据长度小于等于映射区段的大小
        let area_size = (usize::from(self.vpn_range.end()) - usize::from(self.vpn_range.start()))
            << PAGE_SIZE_SHIFT;
        let data_len = data.len();
        assert!(data_len <= area_size);

        let mut start: usize = 0;
        let mut vpn_iter = self.vpn_range.into_iter();

        while let Some(current_vpn) = vpn_iter.next()
            && start < data_len
        {
            let src = &data[start..min(data_len, start + PAGE_SIZE)];
            let dst = &mut pt
                // 此处不校验权限，因为此方法仅用于初始化映射区段的数据
                .translate(current_vpn, None)
                .unwrap()
                .as_mut_type::<[u8; PAGE_SIZE]>();

            // src 在最后一页往往不足一页（ELF 段的 file_size 通常不是页的整数倍），
            // 因此只把 src 长度的前缀拷入目标页；页帧分配时已清零，剩余字节保持为 0。
            dst[..src.len()].copy_from_slice(src);
            start += PAGE_SIZE;
        }
    }
}

/// 从ELF文件收集到的、已通过校验的可加载段
struct LoadSegment {
    /// 段覆盖的页对齐区间 [start_page, end_page)，用于重叠检测
    start_page: usize,
    end_page: usize,
    start_va: usize,
    end_va: usize,
    perm: MemoryMapPermission,
    file_off: usize,
    file_size: usize,
}

/// 内存集
///
/// 内存集是一个逻辑概念，表示一组连续的虚拟内存区域（MemoryMapArea），
/// 每个区域都具有相同的映射类型和权限。内存集通常用于表示一个进程的虚拟内存空间，
/// 或者内核的虚拟内存空间。内存集中的每个区域都可以独立地进行映射和权限设置
pub struct MemorySet {
    page_table: PageTable,
    areas: Vec<MemoryMapArea>,
}

impl MemorySet {
    /// 创建一个新的内存集
    pub fn new() -> Self {
        Self {
            page_table: PageTable::new(),
            areas: Vec::new(),
        }
    }

    /// 将一个映射区段添加到内存集中
    ///
    /// ## 参数
    /// - `area` 映射区段
    /// - `data` 可选的初始化数据，如果提供，将会被写入到映射的物理页中
    fn push(&mut self, mut area: MemoryMapArea, data: Option<&[u8]>) {
        area.map(&mut self.page_table);
        if let Some(data) = data {
            area.copy_data_from(&mut self.page_table, data);
        }
        self.areas.push(area);
    }

    pub fn insert_framed_area(
        &mut self,
        start_va: VirtualAddress,
        end_va: VirtualAddress,
        map_perm: MemoryMapPermission,
        data: Option<&[u8]>,
    ) {
        // WARN: 存在潜在安全问题 - VA范围可能与现有的映射区段重叠，导致数据覆盖或权限冲突
        // WARN: 存在潜在安全问题 - data的长度可能大于VA范围，导致assert失败，进而panic
        self.push(
            MemoryMapArea::new(start_va, end_va, MemoryMapType::Framed, map_perm),
            data,
        );
    }

    fn map_trampoline(&mut self) {
        self.page_table.map(
            VirtualAddress::from(TRAMPOLINE).into(),
            PhysicalAddress::from(linker_symbol_addr!(trap::s_u_trampoline)).into(),
            PTEFlags::R | PTEFlags::X,
        );
    }

    /// 获取内存集的页表的satp值
    pub fn get_satp(&self) -> Satp {
        self.page_table.to_satp()
    }

    /// 将虚拟地址转换为物理地址
    ///
    /// ## 参数
    /// - `va`: 虚拟地址
    /// - `perm_check`: 可选的权限检查，如果提供，则会检查该虚拟地址是否具有指定的权限
    pub fn translate(
        &self,
        va: VirtualAddress,
        perm_check: Option<MemoryMapPermission>,
    ) -> Option<PhysicalAddress> {
        let vpn = va.floor_page(); // 虚拟页号为VA向下取整
        let offset = va.page_offset(); // 页内偏移为VA对页大小取模
        self.page_table
            .translate(vpn, perm_check.map(PTEFlags::from))
            .map(|ppn| PhysicalAddress::from(ppn) + PhysicalAddress::from(offset))
    }

    /// 新建内核内存集
    pub fn new_kernel() -> Self {
        // 此处体现出内核linker.lds中要求4k对齐的作用
        // 因为内核的各个段都是4k对齐的，所以可以直接使用段的起始地址和结束地址来创建映射区段
        unsafe extern "C" {
            safe fn stext();
            safe fn etext();
            safe fn srodata();
            safe fn erodata();
            safe fn sdata();
            safe fn edata();
            safe fn sbss_with_stack();
            safe fn ebss();
            safe fn ekernel();
            //safe fn strampoline();
        }

        let mut mem_set = Self::new();
        // 跳板
        mem_set.map_trampoline();
        // 内核代码段
        info!(
            ".text [{:#x}, {:#x})",
            linker_symbol_addr!(stext),
            linker_symbol_addr!(etext)
        );
        // 内核只读数据段
        info!(
            ".rodata [{:#x}, {:#x})",
            linker_symbol_addr!(srodata),
            linker_symbol_addr!(erodata)
        );
        // 内核数据段
        info!(
            ".data [{:#x}, {:#x})",
            linker_symbol_addr!(sdata),
            linker_symbol_addr!(edata)
        );
        // 内核BSS段
        info!(
            ".bss [{:#x}, {:#x})",
            linker_symbol_addr!(sbss_with_stack),
            linker_symbol_addr!(ebss)
        );
        mem_set.push(
            MemoryMapArea::new(
                (linker_symbol_addr!(stext)).into(),
                (linker_symbol_addr!(etext)).into(),
                MemoryMapType::Identical,
                MemoryMapPermission::R | MemoryMapPermission::X,
            ),
            None,
        );
        mem_set.push(
            MemoryMapArea::new(
                (linker_symbol_addr!(srodata)).into(),
                (linker_symbol_addr!(erodata)).into(),
                MemoryMapType::Identical,
                MemoryMapPermission::R,
            ),
            None,
        );
        mem_set.push(
            MemoryMapArea::new(
                (linker_symbol_addr!(sdata)).into(),
                (linker_symbol_addr!(edata)).into(),
                MemoryMapType::Identical,
                MemoryMapPermission::R | MemoryMapPermission::W,
            ),
            None,
        );
        mem_set.push(
            MemoryMapArea::new(
                (linker_symbol_addr!(sbss_with_stack)).into(),
                (linker_symbol_addr!(ebss)).into(),
                MemoryMapType::Identical,
                MemoryMapPermission::R | MemoryMapPermission::W,
            ),
            None,
        );
        mem_set.push(
            MemoryMapArea::new(
                (linker_symbol_addr!(ekernel)).into(),
                MEM_END_ADDR.into(),
                MemoryMapType::Identical,
                MemoryMapPermission::R | MemoryMapPermission::W,
            ),
            None,
        );
        mem_set
    }

    /// 从ELF文件创建一个新的内存集，并返回用户栈顶地址和程序入口地址
    ///
    /// 建表前会完整校验所有可加载段（范围/越界/溢出/保留区/重叠/入口点），
    /// 任一校验失败返回 Err，不会产生半构建的内存集或将错误推迟到加载中途 panic。
    pub fn from_elf(elf_data: &[u8]) -> Result<(Self, usize, usize), &'static str> {
        let elf = ElfFile::new(elf_data).map_err(|_| "failed to parse ELF")?;
        if elf.header.pt1.magic != [0x7f, 0x45, 0x4c, 0x46] {
            return Err("invalid ELF magic");
        }

        let entry_point = elf.header.pt2.entry_point() as usize;
        let mut entry_in_exec_segment = false;
        let mut segments: Vec<LoadSegment> = Vec::new();

        for i in 0..elf.header.pt2.ph_count() {
            let ph = elf
                .program_header(i)
                .map_err(|_| "failed to read program header")?;
            if ph.get_type().map_err(|_| "invalid program header type")?
                != xmas_elf::program::Type::Load
            {
                continue;
            }
            // 空段不参与映射（Range::new 要求 start < end）
            let mem_size = ph.mem_size() as usize;
            if mem_size == 0 {
                continue;
            }
            let file_size = ph.file_size() as usize;
            if file_size > mem_size {
                return Err("segment file_size exceeds mem_size");
            }
            let file_off = ph.offset() as usize;
            let file_end = file_off
                .checked_add(file_size)
                .ok_or("segment file range overflow")?;
            if file_end > elf_data.len() {
                return Err("segment file range out of bounds");
            }
            let start_va = ph.virtual_addr() as usize;
            let end_va = start_va
                .checked_add(mem_size)
                .ok_or("segment virtual range overflow")?;
            // 用户空间从 0 开始，且不含保留高地址区 [TRAP_CONTEXT, usize::MAX]
            if start_va == 0 || end_va > TRAP_CONTEXT {
                return Err("segment address outside user space");
            }
            let start_page = start_va >> PAGE_SIZE_SHIFT;
            let end_page = end_va.div_ceil(PAGE_SIZE);
            // 页对齐区间重叠会在 PageTable::map 触发重复映射
            if segments
                .iter()
                .any(|s| start_page < s.end_page && s.start_page < end_page)
            {
                return Err("load segments overlap");
            }

            let mut perm = MemoryMapPermission::U;
            let flags = ph.flags();
            if flags.is_read() {
                perm |= MemoryMapPermission::R;
            }
            if flags.is_write() {
                perm |= MemoryMapPermission::W;
            }
            if flags.is_execute() {
                perm |= MemoryMapPermission::X;
            }
            if flags.is_execute() && (start_va..end_va).contains(&entry_point) {
                entry_in_exec_segment = true;
            }
            segments.push(LoadSegment {
                start_page,
                end_page,
                start_va,
                end_va,
                perm,
                file_off,
                file_size,
            });
        }

        if segments.is_empty() {
            return Err("no loadable segment");
        }
        if !entry_in_exec_segment {
            return Err("entry point is not in an executable load segment");
        }

        // 用户栈放在最高段之上，中间留一个 guard page
        let max_end_page = segments.iter().map(|s| s.end_page).max().unwrap();
        let max_end_va = max_end_page << PAGE_SIZE_SHIFT;
        let user_stack_bottom = max_end_va
            .checked_add(PAGE_SIZE)
            .ok_or("user stack bottom overflow")?;
        let user_stack_top = user_stack_bottom
            .checked_add(U_STACK_SIZE)
            .ok_or("user stack top overflow")?;
        if user_stack_top > TRAP_CONTEXT {
            return Err("user stack overlaps the reserved high address region");
        }

        let mut memory_set = Self::new();
        memory_set.map_trampoline();
        for s in segments {
            memory_set.push(
                MemoryMapArea::new(
                    s.start_va.into(),
                    s.end_va.into(),
                    MemoryMapType::Framed,
                    s.perm,
                ),
                Some(&elf_data[s.file_off..s.file_off + s.file_size]),
            );
        }
        memory_set.push(
            MemoryMapArea::new(
                user_stack_bottom.into(),
                user_stack_top.into(),
                MemoryMapType::Framed,
                MemoryMapPermission::R | MemoryMapPermission::W | MemoryMapPermission::U,
            ),
            None,
        );
        memory_set.push(
            MemoryMapArea::new(
                TRAP_CONTEXT.into(),
                TRAMPOLINE.into(),
                MemoryMapType::Framed,
                MemoryMapPermission::R | MemoryMapPermission::W,
            ),
            None,
        );
        Ok((memory_set, user_stack_top, entry_point))
    }
}

lazy_static! {
    pub static ref KERNEL_MEM: UPSafeCell<MemorySet> =
        unsafe { UPSafeCell::new(MemorySet::new_kernel()) };
}
