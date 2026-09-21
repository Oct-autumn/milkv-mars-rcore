use riscv::register::sstatus::{self, FS, SPP, Sstatus};

/// 中断上下文
/// 用于保存中断发生时的处理器状态
#[repr(C)]
pub struct TrapContext {
    /// 保存的通用寄存器 x0-x31
    pub x: [usize; 32],
    /// 保存的 sstatus 寄存器
    pub sstatus: Sstatus,
    /// 保存的 sepc 寄存器
    pub sepc: usize,
    /// 保存的浮点寄存器 f0-f31
    pub f: [usize; 32],
    /// 保存的 fcsr 寄存器
    pub fcsr: usize,

    /* 以下为只读 */
    /// 保存的内核页表根物理页号
    pub k_satp: usize,
    /// 保存的内核栈指针
    pub k_sp: usize,
    /// 保存的内核trap_handler入口地址
    pub trap_handler: usize,
}

impl TrapContext {
    pub fn set_sp(&mut self, sp: usize) {
        self.x[2] = sp;
    }
    pub fn app_init_context(
        entry: usize,
        sp: usize,
        k_satp: usize,
        k_sp: usize,
        trap_handler: usize,
    ) -> Self {
        let mut sstatus = sstatus::read();
        sstatus.set_spp(SPP::User);
        // 显式开启浮点单元（FS=Initial），否则用户程序执行浮点指令会陷入。
        sstatus.set_fs(FS::Initial);
        let mut cx = Self {
            sepc: entry,
            x: [0; 32],
            sstatus,
            f: [0; 32],
            fcsr: 0,
            k_satp,
            k_sp,
            trap_handler,
        };
        cx.set_sp(sp);
        cx
    }
}
