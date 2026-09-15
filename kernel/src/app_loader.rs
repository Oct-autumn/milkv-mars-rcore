use core::arch::asm;

use crate::{
    config::{APP_BASE_ADDRESS, APP_SIZE_LIMIT, KERNEL_STACK_SIZE, MAX_APP_NUM, USER_STACK_SIZE},
    linker_symbol_addr,
    trap::TrapContext,
};

#[repr(align(4096))]
#[derive(Copy, Clone)]
struct KernelStack {
    data: [u8; KERNEL_STACK_SIZE],
}

#[repr(align(4096))]
#[derive(Copy, Clone)]
struct UserStack {
    data: [u8; USER_STACK_SIZE],
}

// 无 MMU 阶段的取舍：KERNEL_STACK/USER_STACK 是全零的不可变 static，LLVM 会把
// 它们放进只读的 .rodata，而 KernelStack::push_context 与运行中的用户程序都会
// 通过裸指针写入。严格来说这是对只读常量内存的写入（UB），当前能正常工作仅
// 因为尚未开启 MMU/分页——物理内存没有只读保护，写入照常落到 DDR 上。一旦开启
// 页表并把 .rodata 映射为只读，这里会立刻在 S 态触发 Store Fault；届时每个进程
// 将拥有各自独立地址空间中的内核栈/用户栈，本实现会被替换。
static KERNEL_STACK: [KernelStack; MAX_APP_NUM] = [KernelStack {
    data: [0; KERNEL_STACK_SIZE],
}; MAX_APP_NUM];

static USER_STACK: [UserStack; MAX_APP_NUM] = [UserStack {
    data: [0; USER_STACK_SIZE],
}; MAX_APP_NUM];

impl KernelStack {
    /// 获取内核栈的栈底地址
    ///
    /// 注：栈底地址是栈的最高地址，栈是向下增长的
    fn get_sp(&self) -> usize {
        self.data.as_ptr() as usize + KERNEL_STACK_SIZE
    }
    /// 将 TrapContext 压入内核栈，并返回 TrapContext 的指针
    pub fn push_context(&self, trap_cx: TrapContext) -> usize {
        let trap_cx_ptr = (self.get_sp() - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
        unsafe {
            *trap_cx_ptr = trap_cx;
        }
        trap_cx_ptr as usize
    }
}

impl UserStack {
    /// 获取用户栈的栈底地址
    ///
    /// 注：栈底地址是栈的最高地址，栈是向下增长的
    fn get_sp(&self) -> usize {
        self.data.as_ptr() as usize + USER_STACK_SIZE
    }
}

pub fn get_num_app() -> usize {
    unsafe extern "C" {
        safe fn _num_app();
    }

    // SAFETY: 这里的 unsafe 是安全的，因为我们确保了 _num_app 符号在链接器脚本中定义，并且它指向一个有效的 usize 值。
    unsafe { (linker_symbol_addr!(_num_app) as *const usize).read_volatile() }
}

pub fn get_app_base(app_id: usize) -> usize {
    APP_BASE_ADDRESS + app_id * APP_SIZE_LIMIT
}

pub fn load_apps() {
    unsafe extern "C" {
        safe fn _num_app();
    }

    // SAFETY: 这里的 unsafe 是安全的，因为我们确保了 _num_app 符号在链接器脚本中定义，并且它指向一个有效的 usize 值。
    unsafe {
        let num_app_ptr = linker_symbol_addr!(_num_app) as *const usize;
        let num_app = num_app_ptr.read_volatile();
        let app_start = core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1);

        for i in 0..num_app {
            let base_i = APP_BASE_ADDRESS + i * APP_SIZE_LIMIT;
            // 清理应用程序区
            (base_i..base_i + APP_SIZE_LIMIT).for_each(|addr| (addr as *mut u8).write_volatile(0));
            // 将应用程序从链接器符号地址复制到应用程序区
            let src = core::slice::from_raw_parts(
                app_start[i] as *const u8,
                app_start[i + 1] - app_start[i],
            );
            let dst = core::slice::from_raw_parts_mut(base_i as *mut u8, src.len());
            dst.copy_from_slice(src);
        }
        asm!("fence.i"); // 这条指令是必要的。它确保在复制应用程序到内存后，指令缓存被刷新，以便 CPU 能够正确地执行新加载的应用程序代码。
    }
}

pub fn init_app_cx(app_id: usize) -> usize {
    KERNEL_STACK[app_id].push_context(TrapContext::app_init_context(
        get_app_base(app_id),
        USER_STACK[app_id].get_sp(),
    ))
}

pub fn get_user_stack_base(app_id: usize) -> usize {
    USER_STACK[app_id].get_sp()
}
