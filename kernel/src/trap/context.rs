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
            k_sp,
            trap_handler,
        };
        cx.set_sp(sp);
        cx
    }
}

// 布局即 ABI：Rust 字段偏移必须与 trap.S 的 .equ 常量逐一对齐。
// 编译期断言把「静默错位」变成「编译失败」，从此增删字段是安全的。
// 对应常量见 trap.S：SSTATUS_OFFSET / SEPC_OFFSET / FP_OFFSET / FCSR_OFFSET /
// K_SP_OFFSET / TRAP_HANDLER_OFFSET。
const _: () = {
    use core::mem::offset_of;
    assert!(offset_of!(TrapContext, x) == 0);
    assert!(offset_of!(TrapContext, sstatus) == 32 * 8);
    assert!(offset_of!(TrapContext, sepc) == 33 * 8);
    assert!(offset_of!(TrapContext, f) == (32 + 2) * 8);
    assert!(offset_of!(TrapContext, fcsr) == (32 + 2 + 32) * 8);
    assert!(offset_of!(TrapContext, k_sp) == (32 + 2 + 32 + 1) * 8);
    assert!(offset_of!(TrapContext, trap_handler) == (32 + 2 + 32 + 2) * 8);
};
