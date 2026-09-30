use core::{
    arch::asm,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use lazy_static::lazy_static;
use riscv::register::satp;

use crate::{debug, sync::UPSafeCell, utils::BitMap};

/// ASID (地址空间标识符) 结构体
pub struct Asid {
    /// ASID 值
    pub value: usize,
    /// ASID 代数
    pub generation: usize,
}

/// RAII 风格的 ASID 回收器，当 Asid 被丢弃时，自动回收 ASID
impl Drop for Asid {
    fn drop(&mut self) {
        // 当 Asid 被丢弃时，回收 ASID
        ASID_ALLOCATOR.exclusive_access().deallocate(self);
    }
}

static ASID_SUPPORTED: AtomicBool = AtomicBool::new(false);

pub fn asid_enabled() -> bool {
    ASID_SUPPORTED.load(Ordering::Relaxed)
}

static ASID_GENERATION: AtomicUsize = AtomicUsize::new(0);

fn asid_generation_increment() {
    ASID_GENERATION.fetch_add(1, Ordering::AcqRel);
}

/// 获取当前 ASID 代数
pub fn asid_generation() -> usize {
    ASID_GENERATION.load(Ordering::Acquire)
}

/// ASID 分配器
pub struct AsidAllocator {
    /// 已分配的 ASID 位图
    allocated: BitMap,
}

impl AsidAllocator {
    /// 创建一个新的 ASID 分配器
    pub fn new() -> Self {
        let asid_len = {
            let len = Self::detect_asid_len();
            if len == 0 {
                debug!("ASID is not supported by the hardware");
                0
            } else {
                // 硬件支持 ASID，设置 ASID_SUPPORTED 为 true
                ASID_SUPPORTED.store(true, Ordering::Relaxed);
                debug!("ASID length: {} bits", len);
                len
            }
        };
        let mut allocated = BitMap::new(1 << asid_len);
        allocated.set(0); // 保留 ASID 0，表示没有 ASID
        Self { allocated }
    }

    /// 检测当前硬件支持的 ASID 长度（单位：位）
    ///
    /// 通过向 satp 寄存器写入一个全1的ASID值，然后读回，来判断硬件支持的 ASID 长度。
    fn detect_asid_len() -> usize {
        let asid_len;
        unsafe {
            let mut satp_val;
            // 读出当前satp，写入一个全1的ASID值，然后读回
            let saved_satp = satp::read();
            satp_val = saved_satp;
            satp_val.set_asid(usize::MAX);
            satp::write(satp_val);
            satp_val = satp::read();
            // 计算ASID长度：从最低位开始，统计连续的1的数量
            asid_len = satp_val.asid().trailing_ones() as usize;
            // 恢复原来的satp值
            satp::write(saved_satp);

            // 此处执行一次sfence.vma，确保TLB刷新，防止后续访存异常
            asm!("sfence.vma");
        }
        asid_len
    }

    /// 分配一个新的 ASID
    pub fn allocate(&mut self) -> Asid {
        if !asid_enabled() {
            // 如果硬件不支持ASID，则始终返回ASID 0
            return Asid {
                value: 0,
                generation: asid_generation(),
            };
        }
        if let Some(asid) = self.allocated.find_first_unset() {
            self.allocated.set(asid);
            Asid {
                value: asid,
                generation: asid_generation(),
            }
        } else {
            // 如果没有可用的 ASID，则清空已分配的 ASID 位图，并重新分配
            asid_generation_increment();
            self.allocated.reset();
            self.allocated.set(0); // 依旧保留 ASID 0
            unsafe {
                // 全量刷新 TLB，确保所有旧的 ASID 都被清除
                asm!("sfence.vma");
            }

            let asid = self.allocated.find_first_unset().unwrap();
            Asid {
                value: asid,
                generation: asid_generation(),
            }
        }
    }

    /// 回收一个 ASID
    pub fn deallocate(&mut self, asid: &Asid) {
        if !asid_enabled() {
            // 如果硬件不支持ASID，则无需回收
            return;
        }
        assert!(asid.value != 0, "ASID 0 cannot be deallocated"); // ASID 0 是保留的，不能回收
        if asid.generation == asid_generation() {
            unsafe {
                // 刷新TLB，确保该ASID对应的页表项不在清除后使用
                // G类页表项不在此列
                asm!("sfence.vma zero, {asid}", asid = in(reg) asid.value);
            }
            self.allocated.clear(asid.value);
        }
    }
}

lazy_static! {
    /// 全局 ASID 分配器实例
    pub static ref ASID_ALLOCATOR: UPSafeCell<AsidAllocator> = unsafe {
        UPSafeCell::new(AsidAllocator::new())
    };
}
