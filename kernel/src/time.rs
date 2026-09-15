use crate::config;
use riscv::register::time;

/// 通过读 time 寄存器，获取当前时间（单位：微秒）
pub fn get_time() -> usize {
    let time = time::read();
    (time / (config::MTIME_FREQUENCY / 1000000)) as usize
}
