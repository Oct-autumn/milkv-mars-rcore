use core::{cmp::min, mem::size_of};

use alloc::vec::Vec;

use crate::{
    config::PAGE_SIZE,
    error, info,
    mem::{MemoryMapPermission, VirtualAddress, VirtualPageNumber},
    task::{
        exit_current_task_and_run_next, get_current_task, suspend_current_task_and_run_next,
        translate_current_va,
    },
    time::RunTime,
    utils::Range,
    warn,
};

/// 执行 exit 系统调用
///
/// - code: 退出码
pub fn sys_exit(code: i32) -> ! {
    // 先取统计值再打印，避免把“打印退出信息”本身的耗时也算进内核态时间
    let (user_time, kernel_time) = crate::task::current_task_times();
    if code == 0 {
        info!("Program exited with code 0.");
    } else {
        warn!("Program exited with code {}.", code);
    }
    info!(
        "Task {} run time: user = {}s, kernel = {}s",
        get_current_task(),
        RunTime(user_time),
        RunTime(kernel_time)
    );
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
    let ts_addr = ts as usize;
    let ts_start_va = VirtualAddress::from(ts_addr);
    let ts_end_va = VirtualAddress::from(ts_addr.saturating_add(size_of::<TimeVal>()));

    let time = crate::time::get_time();
    let t_val = TimeVal {
        sec: time / 1_000_000,
        usec: time % 1_000_000,
    };
    let t_val_bytes = unsafe {
        core::slice::from_raw_parts(&t_val as *const TimeVal as *const u8, size_of::<TimeVal>())
    };

    // 检查每个页是否在当前任务的用户栈或应用区内
    // 顺便将每个页的虚拟地址翻译为物理地址，以便后续使用
    let vpn_range =
        Range::<VirtualPageNumber>::new(ts_start_va.floor_page(), ts_end_va.ceil_page(), |va| {
            va + VirtualPageNumber::from(1)
        });
    let mut buf_ppn_list = Vec::new();

    for vpn in vpn_range {
        let va = vpn.into();
        if let Some(pa) =
            translate_current_va(va, Some(MemoryMapPermission::U | MemoryMapPermission::W))
        {
            buf_ppn_list.push(pa.floor_page());
        } else {
            error!(
                "sys_get_time: *ts is not in user space: {:#x}",
                usize::from(va)
            );
            return -1;
        }
    }

    let mut page_offset = ts_start_va.page_offset();
    let mut left_len = size_of::<TimeVal>();

    for ppn in buf_ppn_list {
        let write_len = min(PAGE_SIZE - page_offset, left_len);
        let slice = &mut ppn.as_raw_page()[page_offset..page_offset + write_len];

        slice.copy_from_slice(
            &t_val_bytes
                [size_of::<TimeVal>() - left_len..size_of::<TimeVal>() - left_len + write_len],
        );

        page_offset = 0;
        left_len -= write_len;
    }
    0
}
