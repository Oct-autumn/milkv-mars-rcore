use core::mem::{align_of, size_of};

use crate::{
    app_loader::{get_app_base, get_user_stack_base},
    config::{APP_SIZE_LIMIT, USER_STACK_SIZE},
    error, info,
    task::{exit_current_task_and_run_next, get_current_task, suspend_current_task_and_run_next},
    warn,
};

/// 执行 exit 系统调用
///
/// - code: 退出码
pub fn sys_exit(code: i32) -> ! {
    if code == 0 {
        info!("Program exited with code 0.");
    } else {
        warn!("Program exited with code {}.", code);
    }
    exit_current_task_and_run_next();
}

/// 执行 yield 系统调用
///
/// - 返回值: isize, 0表示成功
pub fn sys_yield() -> isize {
    suspend_current_task_and_run_next();
    0
}

#[repr(C)]
pub struct TimeVal {
    sec: usize,
    usec: usize,
}

/// 获取系统时间
///
/// - ts: 指向用户态 TimeVal 结构的指针，用于存储获取到的时间
/// - _tz: 时区信息，目前未使用
/// - 返回值: 成功返回 0；ts 非法（空/越界/未对齐）返回 -1
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    let current_task_id = get_current_task();
    let u_stack_base = get_user_stack_base(current_task_id);
    let app_base = get_app_base(current_task_id);

    // 与 sys_write 保持一致的边界检查：校验整个 [ts, ts + size_of::<TimeVal>())
    // 落在当前任务的用户栈或应用区内。当前无 MMU（satp=0），内核与用户共享同一
    // 物理地址空间，这是唯一可用的隔离手段，否则用户可传入内核地址覆写内核数据。
    let ts_start = ts as usize;
    let Some(ts_end) = ts_start.checked_add(size_of::<TimeVal>()) else {
        error!("sys_get_time: pointer overflow: {ts:p}");
        return -1;
    };
    let in_user_stack = ts_start >= u_stack_base - USER_STACK_SIZE && ts_end <= u_stack_base;
    let in_app = ts_start >= app_base && ts_end <= app_base + APP_SIZE_LIMIT;
    // TimeVal 由两个 usize 组成，未对齐的裸指针写入属未定义行为，需一并拒绝。
    let aligned = ts_start.is_multiple_of(align_of::<TimeVal>());
    if !((in_user_stack || in_app) && aligned) {
        error!("sys_get_time: invalid TimeVal pointer: {ts:p}");
        return -1;
    }

    let time = crate::time::get_time();
    unsafe {
        *ts = TimeVal {
            sec: time / 1_000_000,
            usec: time % 1_000_000,
        };
    }
    0
}
