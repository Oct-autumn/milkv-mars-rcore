mod fs;
mod process;

use crate::error;

use self::{
    fs::sys_write,
    process::{sys_exit, sys_yield},
};

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;
const SYS_YIELD: usize = 124;

pub fn syscall(id: usize, args: [usize; 3]) -> isize {
    match id {
        SYS_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYS_EXIT => sys_exit(args[0] as i32),
        SYS_YIELD => sys_yield(),
        _ => {
            error!("Unknown syscall id: {}", id);
            sys_exit(-1);
        }
    }
}
