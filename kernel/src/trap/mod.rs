mod context;

pub use context::TrapContext;
use core::arch::global_asm;
use riscv::{
    interrupt::{Exception, Interrupt, Trap},
    register::{
        scause,
        sstatus::{self, SPP},
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
            // 因此这里区分 access fault(cause 7) 与 page fault，并打印出错地址与来源特权级。
            let from = match sstatus::read().spp() {
                SPP::User => "U",
                SPP::Supervisor => "S",
            };
            error!(
                "Store fault from {}-mode: scause={:?}, stval={:#x}, sepc={:#x}",
                from, trap, stval, cx.sepc
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
    cx
}
