use crate::config;
use riscv::register::time;

/// 通过读 time 寄存器，获取当前时间（单位：微秒）
pub fn get_time() -> usize {
    let time = time::read();
    (time / (config::MTIME_FREQUENCY / 1000000)) as usize
}

/// 设置时钟中断定时器
pub fn set_next_timer(timer_interval: usize) {
    let current_time = time::read();
    let next_time = current_time + (config::MTIME_FREQUENCY / 1000000) * timer_interval;
    if sbi::timer::set_timer(next_time as u64).is_err() {
        panic!("Set timer failed!");
    }
}
