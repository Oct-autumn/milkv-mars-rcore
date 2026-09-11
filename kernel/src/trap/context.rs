use riscv::register::sstatus::{self, FS, SPP, Sstatus};

/// 中断上下文
/// 用于保存中断发生时的处理器状态
///
/// 注意：这里只保存通用寄存器与 sstatus/sepc，**不保存浮点寄存器**（f0~f31/fcsr）。
/// 因此内核在任何 trap 处理路径中都不得执行硬浮点指令，否则会破坏用户程序的浮点状态
/// （内核日志 log.rs 已刻意改用整数运算来满足这一约束）。
/// 若将来内核需要浮点，或需要支持"trap 后继续运行使用了浮点的用户程序"，
/// 必须在本结构体中加入浮点寄存器，并在 trap.S 中同步保存/恢复，同时处理 sstatus.FS。
#[repr(C)]
pub struct TrapContext {
    pub x: [usize; 32],
    pub sstatus: Sstatus,
    pub sepc: usize,
}

impl TrapContext {
    pub fn set_sp(&mut self, sp: usize) {
        self.x[2] = sp;
    }
    pub fn app_init_context(entry: usize, sp: usize) -> Self {
        let mut sstatus = sstatus::read();
        sstatus.set_spp(SPP::User);
        // 显式开启浮点单元（FS=Initial），否则用户程序执行浮点指令会陷入。
        // 内核自身不使用浮点，故无需在此保存/恢复浮点状态。
        sstatus.set_fs(FS::Initial);
        let mut cx = Self {
            sepc: entry,
            x: [0; 32],
            sstatus,
        };
        cx.set_sp(sp);
        cx
    }
}
