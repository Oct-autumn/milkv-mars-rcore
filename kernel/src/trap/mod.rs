mod context;

pub use context::TrapContext;
use core::arch::global_asm;
use riscv::{
    interrupt::{Exception, Interrupt, Trap},
    register::{
        scause, sscratch,
        sstatus::SPP,
        stval,
        stvec::{self, TrapMode},
    },
};

use crate::{error, linker_symbol_addr, sys_call::syscall, task::exit_current_task_and_run_next};

global_asm!(include_str!("trap.S"));

pub fn init() {
    unsafe extern "C" {
        fn __alltraps();
    }

    unsafe {
        stvec::write(stvec::Stvec::new(
            linker_symbol_addr!(__alltraps),
            TrapMode::Direct,
        ));
        // sscratch 约定：U 态运行期间存放当前任务的内核栈顶；内核（S 态）运行
        // 期间固定为 0，供 __alltraps 判别 trap 来源。这里显式清零，避免依赖
        // 复位/bootloader 遗留的随机值导致 U 态被误判为 S 态。
        sscratch::write(0);
    }
}

/// Trap处理函数：负责处理各式中断/异常
///
/// - cx: 中断上下文
/// - 返回值: 处理后的中断上下文
#[unsafe(no_mangle)]
pub fn trap_handler(cx: &mut TrapContext) -> &mut TrapContext {
    let scause = scause::read();
    let stval = stval::read();

    // 来源特权级取 TrapContext 中的副本而非硬件 sstatus：嵌套 trap 返回后硬件
    // SPP 已被内层 sret 改写，只有副本始终代表本次 trap 的来源。
    if let SPP::Supervisor = cx.sstatus.spp() {
        // 内核态全程关中断（进入 trap 时硬件清 SIE，内核从不 set_sie），因此来自
        // S 态的 trap 只可能是内核自身的同步异常，即内核 bug；此时当前任务并非
        // 出错方，不能按用户程序故障处理。
        panic!(
            "Trap from S-mode (kernel bug): {:?}, stval={:#x}, sepc={:#x}",
            scause.cause(),
            stval,
            cx.sepc
        );
    }

    // 计时打点（E2）：trap 只可能来自 U 态，先结算本段用户态执行时间
    crate::task::account_trap_entry();

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
        Trap::Exception(Exception::StoreFault) | Trap::Exception(Exception::StorePageFault) => {
            // 无 MMU（satp=0）时不可能出现真正的 PageFault（cause 15）；
            // 因此这里区分 access fault(cause 7) 与 page fault，并打印出错地址。
            error!(
                "Store fault: scause={:?}, stval={:#x}, sepc={:#x}",
                trap, stval, cx.sepc
            );
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
    cx
}
