use crate::config::{
    APP_BASE_ADDRESS, APP_SIZE_LIMIT, KERNEL_STACK_SIZE, MAX_APP_NUM, USER_STACK_SIZE,
};
use crate::sbi_call::shutdown;
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use crate::{debug, info, linker_symbol_addr, print};
use crate::{time, warn};
use core::arch::asm;
use lazy_static::*;
use sbi::system_reset::{ResetReason, ResetType};

#[repr(align(4096))]
struct KernelStack {
    data: [u8; KERNEL_STACK_SIZE],
}

#[repr(align(4096))]
struct UserStack {
    data: [u8; USER_STACK_SIZE],
}

// 当前这样做是安全的（无 MMU 阶段的取舍）：
// 这两个栈是全零的不可变 static，LLVM 会把它们放进只读的 .rodata；而
// KernelStack::push_context 与用户程序都会通过裸指针写入。严格来说这是对
// 只读常量内存的写入（UB），但它现在能正常工作，是因为尚未开启 MMU/分页：
// 物理内存没有只读保护，写入照常落到 DDR 上。一旦在保留 batch 的前提下开启
// 页表并把 .rodata 映射为只读，这里会立刻在 S 态触发 Store Fault。
// 启用分页时 batch 会被替换——届时每个进程拥有各自的内核栈/用户栈并落在其
// 独立地址空间中，这段"单内核栈 + 单用户栈"的实现不会保留，故无需现在改。
static KERNEL_STACK: KernelStack = KernelStack {
    data: [0; KERNEL_STACK_SIZE],
};
static USER_STACK: UserStack = UserStack {
    data: [0; USER_STACK_SIZE],
};

impl KernelStack {
    fn get_sp(&self) -> usize {
        self.data.as_ptr() as usize + KERNEL_STACK_SIZE
    }
    pub fn push_context(&self, cx: TrapContext) -> &'static mut TrapContext {
        // SAFETY: 无 MMU 时 .rodata 对应的物理内存仍可写，详见 KERNEL_STACK 定义处的说明。
        let cx_ptr = (self.get_sp() - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
        unsafe {
            *cx_ptr = cx;
        }
        unsafe { cx_ptr.as_mut().unwrap() }
    }
}

impl UserStack {
    fn get_sp(&self) -> usize {
        self.data.as_ptr() as usize + USER_STACK_SIZE
    }
}

struct AppManager {
    num_app: usize,
    current_app: usize,
    app_start: [usize; MAX_APP_NUM + 1],
}

impl AppManager {
    pub fn print_app_info(&self) {
        debug!("num_app = {}", self.num_app);
        for i in 0..self.num_app {
            debug!(
                "app_{} [{:#x}, {:#x})",
                i,
                self.app_start[i],
                self.app_start[i + 1]
            );
        }
    }

    fn load_app(&self, app_id: usize) {
        if app_id >= self.num_app {
            warn!("All applications completed!");

            for i in 0..10 {
                print!("\r> Shutdown in {: >2} seconds...", 10 - i);
                time::busy_wait_sleep(1000);
            }
            print!("\r> Shutdown now ...\n");

            shutdown(ResetType::Shutdown, ResetReason::NoReason);
        }
        info!("Loading app_{}", app_id);
        unsafe {
            // clear app area
            core::slice::from_raw_parts_mut(APP_BASE_ADDRESS as *mut u8, APP_SIZE_LIMIT).fill(0);
            let app_src = core::slice::from_raw_parts(
                self.app_start[app_id] as *const u8,
                self.app_start[app_id + 1] - self.app_start[app_id],
            );
            let app_dst =
                core::slice::from_raw_parts_mut(APP_BASE_ADDRESS as *mut u8, app_src.len());
            app_dst.copy_from_slice(app_src);
            // Memory fence about fetching the instruction memory
            // It is guaranteed that a subsequent instruction fetch must
            // observes all previous writes to the instruction memory.
            // Therefore, fence.i must be executed after we have loaded
            // the code of the next app into the instruction memory.
            // See also: riscv non-priv spec chapter 3, 'Zifencei' extension.
            asm!("fence.i");
        }
    }

    pub fn get_current_app(&self) -> usize {
        self.current_app
    }

    pub fn move_to_next_app(&mut self) {
        self.current_app += 1;
    }
}

lazy_static! {
    static ref APP_MANAGER: UPSafeCell<AppManager> = unsafe {
        UPSafeCell::new({
            unsafe extern "C" {
                safe fn _num_app();
            }
            let num_app_ptr = linker_symbol_addr!(_num_app) as *const usize;
            let num_app = num_app_ptr.read_volatile();
            let mut app_start: [usize; MAX_APP_NUM + 1] = [0; MAX_APP_NUM + 1];
            let app_start_raw: &[usize] =
                core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1);
            app_start[..=num_app].copy_from_slice(app_start_raw);
            AppManager {
                num_app,
                current_app: 0,
                app_start,
            }
        })
    };
}

/// init batch subsystem
pub fn init() {
    print_app_info();
}

/// print apps info
pub fn print_app_info() {
    APP_MANAGER.exclusive_access().print_app_info();
}

/// run next app
pub fn run_next_app() -> ! {
    let mut app_manager = APP_MANAGER.exclusive_access();
    let current_app = app_manager.get_current_app();
    app_manager.load_app(current_app);
    app_manager.move_to_next_app();
    drop(app_manager);
    // before this we have to drop local variables related to resources manually
    // and release the resources
    unsafe extern "C" {
        unsafe fn __restore(cx_addr: usize);
    }
    unsafe {
        __restore(KERNEL_STACK.push_context(TrapContext::app_init_context(
            APP_BASE_ADDRESS,
            USER_STACK.get_sp(),
        )) as *const _ as usize);
    }
    panic!("Unreachable in batch::run_current_app!");
}

/// 获取当前运行的应用程序的用户栈基址
pub fn get_current_user_stack_base() -> usize {
    USER_STACK.data.as_ptr() as usize
}
