mod context;

pub use context::TrapContext;

use core::arch::{asm, global_asm};
use riscv::{
    interrupt::{Exception, Interrupt, Trap},
    register::{
        scause, stval,
        stvec::{self, TrapMode},
    },
};

use crate::{
    config::{TRAMPOLINE, TRAP_CONTEXT},
    error, linker_symbol_addr,
    sys_call::syscall,
    task::{exit_current_task_and_run_next, get_current_satp, get_current_trap_cx},
};

global_asm!(include_str!("trap.S"));

unsafe extern "C" {
    /// U态的 trap 处理代码
    pub safe fn s_u_trampoline();
}

/// Trap处理函数：负责处理各式中断/异常
///
/// - cx: 中断上下文
/// - 返回值: 处理后的中断上下文
#[unsafe(no_mangle)]
pub fn trap_handler() -> ! {
    set_kernel_trap_entry();

    // 计时打点（E2）：trap 只可能来自 U 态，先结算本段用户态执行时间
    crate::task::account_trap_entry();

    let cx = get_current_trap_cx();
    let scause = scause::read();
    let stval = stval::read();

    let trap: Trap<Interrupt, Exception> = match scause.cause().try_into() {
        Ok(trap) => trap,
        Err(_) => {
            error!(
                "Unsupported trap {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
            exit_current_task_and_run_next();
        }
    };
    match trap {
        Trap::Exception(Exception::UserEnvCall) => {
            // 对于用户态的系统调用，我们要做两件事：
            // 1. 将sepc指向下一条指令，这样调用返回后用户程序就可以继续执行了
            // 2. 调用syscall函数处理系统调用
            cx.sepc += 4;
            cx.x[10] = syscall(cx.x[17], [cx.x[10], cx.x[11], cx.x[12]]) as usize;
        }
        Trap::Exception(Exception::StorePageFault) => {
            // 写缺页异常，说明用户程序试图写入一个未映射的虚拟地址
            error!("Store page fault: stval={:#x}, sepc={:#x}", stval, cx.sepc);
            exit_current_task_and_run_next();
        }
        Trap::Exception(Exception::LoadPageFault) => {
            // 读缺页异常，说明用户程序试图读取一个未映射的虚拟地址
            error!("Load page fault: stval={:#x}, sepc={:#x}", stval, cx.sepc);
            exit_current_task_and_run_next();
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            error!("IllegalInstruction in application, kernel killed it.");
            exit_current_task_and_run_next();
        }
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            crate::time::set_next_timer(crate::config::STIMER_INTERVAL);
            crate::task::suspend_current_task_and_run_next();
        }
        _ => {
            error!(
                "Unsupported trap {:?}: scause={:?}, stval = {:#x}!",
                trap, scause, stval
            );
            exit_current_task_and_run_next();
        }
    }
    // 计时打点（E3）：返回用户态前结算本段内核态执行时间
    crate::task::account_trap_exit();
    u_trap_return()
}

/// 从内核态返回用户态
pub fn u_trap_return() -> ! {
    set_user_trap_entry();
    let trap_cx_ptr = TRAP_CONTEXT;
    let user_satp = get_current_satp();
    // __u_restore 的代码位于 内核的 trampoline 段（TRAMPOLINE）中
    // 由于 __u_restore 的代码在编译时无法确定最终的虚拟地址，因此我们在运行时计算其虚拟地址：
    // __u_restore 的虚拟地址 = TRAMPOLINE + (__u_restore 的链接时偏移 - __u_alltraps 的链接时偏移)
    unsafe extern "C" {
        unsafe fn __u_alltraps();
        unsafe fn __u_restore();
    }
    let restore_fn_va =
        linker_symbol_addr!(__u_restore) - linker_symbol_addr!(__u_alltraps) + TRAMPOLINE;

    unsafe {
        // 在返回用户态前，刷新指令缓存，确保 __u_restore 的指令被正确加载
        // 执行 __u_restore 时，a0 = trap_cx_ptr, a1 = user_satp
        asm!(
            "fence.i",
            "jr {restore_fn_va}",
            restore_fn_va = in(reg) restore_fn_va,
            in("a0") trap_cx_ptr,
            in("a1") user_satp,
            options(noreturn)
        )
    }
}

/// 设置用户态的 Trap 入口地址为 trampoline 段的 __u_alltraps
fn set_user_trap_entry() {
    unsafe { stvec::write(stvec::Stvec::new(TRAMPOLINE, TrapMode::Direct)) }
}

/// 设置内核态的 Trap 入口地址为 __k_alltraps
///
/// 内核态 trap 一律运行在恒等映射的内核地址空间中（内核页表把 [stext, etext)
/// 恒等映射为 R|X），因此直接使用 __k_alltraps 的链接地址即可。这里不能沿用
/// 「TRAMPOLINE + (__k_alltraps - __u_alltraps)」的偏移算法：u/k 两段 trampoline
/// 各占独立的一页，K 段在高地址区并未被映射，且该偏移还会使地址发生 64 位回绕。
fn set_kernel_trap_entry() {
    unsafe extern "C" {
        unsafe fn __k_alltraps();
    }
    unsafe {
        stvec::write(stvec::Stvec::new(
            linker_symbol_addr!(__k_alltraps),
            TrapMode::Direct,
        ))
    }
}
