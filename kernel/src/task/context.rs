use core::arch::global_asm;

use crate::linker_symbol_addr;

#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct TaskContext {
    /// __switch return address
    ra: usize,
    /// __switch stack pointer
    sp: usize,
    /// callee saved registers
    s: [usize; 12],
}

impl TaskContext {
    /// 创建一个全零的 TaskContext
    pub const fn zero_init() -> Self {
        TaskContext {
            ra: 0,
            sp: 0,
            s: [0; 12],
        }
    }

    /// 创建一个用于恢复的 TaskContext
    pub fn goto_restore(k_stack_ptr: usize) -> Self {
        unsafe extern "C" {
            fn __restore();
        }
        TaskContext {
            ra: linker_symbol_addr!(__restore),
            sp: k_stack_ptr,
            s: [0; 12],
        }
    }
}

global_asm!(include_str!("switch.S"));

unsafe extern "C" {
    pub unsafe fn __switch(current_task_cx: *mut TaskContext, next_task_cx: *const TaskContext);
}
