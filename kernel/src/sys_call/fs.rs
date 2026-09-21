use core::cmp::min;

use alloc::vec::Vec;

use crate::config::PAGE_SIZE;
use crate::mem::{MemoryMapPermission, VirtualAddress, VirtualPageNumber};
use crate::task::translate_current_va;
use crate::utils::{Range, Utf8StreamValidator};
use crate::{console, error};

const FD_STDOUT: usize = 1;

/// 执行 write 系统调用
///
/// - fd: 文件描述符
/// - buf: 缓冲区指针
/// - len: 缓冲区长度
/// - 返回值: 成功时返回写入的字节数，失败（非法缓冲区/非法 UTF-8/不支持 fd）返回 -1
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    // 空缓冲区直接返回，避免 floor(buf)==ceil(buf) 命中 Range::new 的 start<end 断言。
    if len == 0 {
        return 0;
    }
    let buf_addr = buf as usize;
    let buf_start_va = VirtualAddress::from(buf_addr);
    let buf_end_va = VirtualAddress::from(buf_addr.saturating_add(len));

    // 检查每个页是否在当前任务的用户栈或应用区内
    // 顺便将每个页的虚拟地址翻译为物理地址，以便后续使用
    let vpn_range =
        Range::<VirtualPageNumber>::new(buf_start_va.floor_page(), buf_end_va.ceil_page(), |va| {
            va + VirtualPageNumber::from(1)
        });
    let mut buf_ppn_list = Vec::new();

    for vpn in vpn_range {
        let va = vpn.into();
        if let Some(pa) =
            translate_current_va(va, Some(MemoryMapPermission::U | MemoryMapPermission::R))
        {
            buf_ppn_list.push(pa.floor_page());
        } else {
            error!(
                "sys_write: buffer is not in user space: {:#x}",
                usize::from(va)
            );
            return -1;
        }
    }

    let buf_ppn_list = buf_ppn_list; // 让 buf_ppn_list 不可变，避免后续误修改
    let start_offset = buf_start_va.page_offset();

    match fd {
        FD_STDOUT => {
            // 先整段校验 UTF-8（跨页状态由 validator 维护）
            let mut validator = Utf8StreamValidator::new();
            let mut v_offset = start_offset;
            let mut v_left = len;
            for ppn in &buf_ppn_list {
                let check_len = min(PAGE_SIZE - v_offset, v_left);
                let slice = &ppn.as_raw_page()[v_offset..v_offset + check_len];

                if !validator.feed(slice) {
                    error!("sys_write: buffer is not valid UTF-8");
                    return -1;
                }

                v_offset = 0;
                v_left -= check_len;
            }
            if !validator.finished() {
                error!("sys_write: buffer is not valid UTF-8");
                return -1;
            }

            // 校验通过后再输出；输出遍历使用独立的偏移/剩余长度
            let mut w_offset = start_offset;
            let mut w_left = len;
            for ppn in &buf_ppn_list {
                let write_len = min(PAGE_SIZE - w_offset, w_left);
                let slice = &ppn.as_raw_page()[w_offset..w_offset + write_len];

                console::write_bytes(slice);

                w_offset = 0;
                w_left -= write_len;
            }
        }
        _ => {
            error!("Unsupported fd: {fd}");
            return -1;
        }
    }

    len as isize
}
