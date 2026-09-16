use crate::config;
use riscv::register::time;

/// 通过读 time 寄存器，获取当前时间（单位：微秒）
pub fn get_time() -> usize {
    let time = time::read();
    (time / (config::MTIME_FREQUENCY / 1000000)) as usize
}

/// 以 `秒.毫秒`（保留 3 位小数，如 `1.234s`）显示一段以微秒为单位的时间。
///
/// 与 `log::LogTimestamp` 一样刻意使用整数运算而非浮点格式化。一方面整数运算更快，
/// 另一方面浮点数格式化输出会增加内核体积（浮点数格式化依赖 libcore 中的 `float` 模块，约 20KB）。
pub struct RunTime(pub usize);

impl core::fmt::Display for RunTime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // 先四舍五入到毫秒，再拆成 秒.毫秒
        let total_ms = (self.0 + 500) / 1000;
        write!(f, "{}.{:03}", total_ms / 1000, total_ms % 1000)
    }
}

/// 设置时钟中断定时器
pub fn set_next_timer(timer_interval: usize) {
    let current_time = time::read();
    let next_time = current_time + (config::MTIME_FREQUENCY / 1000000) * timer_interval;
    if sbi::timer::set_timer(next_time as u64).is_err() {
        panic!("Set timer failed!");
    }
}
