// 四级Log macro：Error > Warn > Info > Debug

use core::fmt;

/// 日志时间戳（微秒），以整数运算显示为 `秒.毫秒`。
/// 通过 time::get_time() 获取时间（微秒），格式化为"秒.毫秒"
///
/// 注意：这里刻意使用整数运算而非浮点，使内核不执行任何硬浮点指令。
/// 原因见 trap/context.rs：TrapContext 目前不保存浮点寄存器，内核若在
/// trap 处理路径中用到浮点，就会破坏用户程序的 f0~f31/fcsr。
pub(crate) struct LogTimestamp(pub usize);

impl fmt::Display for LogTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 先四舍五入到毫秒，再拆成 秒.毫秒，行为与原来的 {:.3} 一致
        let total_ms = (self.0 + 500) / 1000;
        write!(f, "{}.{:03}", total_ms / 1000, total_ms % 1000)
    }
}

macro_rules! format_log {
    ($level: literal, $inner: expr) => {
        format_args!(
            "{}[ {}] | {: >5} | {}\x1b[0m\n",
            match $level {
                "ERROR" => "\x1b[1;31m",
                "WARN" => "\x1b[1;33m",
                "INFO" => "\x1b[1;32m",
                "DEBUG" => "\x1b[1;36m",
                _ => "\x1b[0m",
            },
            $crate::log::LogTimestamp($crate::time::get_time()),
            $level,
            $inner
        )
    };
}

pub(crate) use format_log;

#[macro_export]
macro_rules! error {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print($crate::log::format_log!(
            "ERROR",
            // 这里使用 format_args! 保留用户传来的格式字符串为字面量，以便在外层格式化时高亮显示
            format_args!($fmt $(, $($arg)+)?)
        ));
    }
}

#[macro_export]
macro_rules! warn {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print($crate::log::format_log!(
            "WARN",
            // 这里使用 format_args! 保留用户传来的格式字符串为字面量，以便在外层格式化时高亮显示
            format_args!($fmt $(, $($arg)+)?)
        ));
    }
}

#[macro_export]
macro_rules! info {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print($crate::log::format_log!(
            "INFO",
            // 这里使用 format_args! 保留用户传来的格式字符串为字面量，以便在外层格式化时高亮显示
            format_args!($fmt $(, $($arg)+)?)
        ));
    }
}

#[macro_export]
macro_rules! debug {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print($crate::log::format_log!(
            "DEBUG",
            // 这里使用 format_args! 保留用户传来的格式字符串为字面量，以便在外层格式化时高亮显示
            format_args!($fmt $(, $($arg)+)?)
        ));
    }
}
