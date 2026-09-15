mod fs;
mod process;

use crate::{error, sys_call::process::sys_get_time};

use self::{
    fs::sys_write,
    process::{TimeVal, sys_exit, sys_yield},
};

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;
const SYS_YIELD: usize = 124;
const SYS_GET_TIME: usize = 169;

pub fn syscall(id: usize, args: [usize; 3]) -> isize {
    match id {
        SYS_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYS_EXIT => sys_exit(args[0] as i32),
        SYS_YIELD => sys_yield(),
        SYS_GET_TIME => sys_get_time(args[0] as *mut TimeVal, args[1]),
        _ => {
            error!("Unknown syscall id: {}", id);
            sys_exit(-1);
        }
    }
}
