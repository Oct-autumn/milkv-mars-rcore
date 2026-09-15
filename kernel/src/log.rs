// 五级Log macro：Error > Warn > Info > Debug > Trace（数值越大越严重）
//
// 输出门槛由 config.toml 的 log_level 经 build.rs 注入为编译期常量（LOG_LEVEL）。
// 低于门槛的日志分支会被常量折叠整体消除，不产生运行时开销，
// 其字符串字面量也不会进入内核镜像。

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

/// 日志严重度：数值越大越严重，供宏做编译期比较。
pub const LEVEL_ERROR: u8 = 5;
pub const LEVEL_WARN: u8 = 4;
pub const LEVEL_INFO: u8 = 3;
pub const LEVEL_DEBUG: u8 = 2;
pub const LEVEL_TRACE: u8 = 1;

/// 编译期逐字节字符串比较。
///
/// 说明：`const fn` 目前不能对 `str` 做 `match`（`const_cmp` 尚未稳定），
/// 因此手写比较以在编译期完成等级名到严重度的映射。
const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// 将 config.toml 的 log_level 名称映射为严重度，编译期求值。
///
/// build.rs 已做白名单校验；此处对未知值兜底为 debug，保证缺省行为。
pub const fn rank(name: &str) -> u8 {
    if str_eq(name, "error") {
        LEVEL_ERROR
    } else if str_eq(name, "warn") {
        LEVEL_WARN
    } else if str_eq(name, "info") {
        LEVEL_INFO
    } else if str_eq(name, "trace") {
        LEVEL_TRACE
    } else {
        LEVEL_DEBUG
    }
}

/// 当前生效的日志门槛：仅输出严重度不低于该值的日志。
pub const LOG_LEVEL: u8 = rank(crate::config::LOG_LEVEL_NAME);

macro_rules! format_log {
    ($level: literal, $inner: expr) => {
        format_args!(
            "{}[ {}] | {: >5} | {}\x1b[0m\n",
            match $level {
                "ERROR" => "\x1b[1;31m",
                "WARN" => "\x1b[1;33m",
                "INFO" => "\x1b[1;32m",
                "DEBUG" => "\x1b[1;36m",
                "TRACE" => "\x1b[1;90m",
                _ => "\x1b[0m",
            },
            $crate::log::LogTimestamp($crate::time::get_time()),
            $level,
            $inner
        )
    };
}

pub(crate) use format_log;

/// 内部共用门控宏：仅当当前门槛不高于 `$rank` 时才输出。
///
/// `LOG_LEVEL` 与 `$rank` 均为编译期常量，条件可被折叠：
/// 被关闭的等级连同其参数、格式化与时间戳读取一并被消除。
#[doc(hidden)]
#[macro_export]
macro_rules! __log {
    ($rank:expr, $level:literal, $fmt:literal $(, $($arg:tt)+)?) => {
        if $crate::log::LOG_LEVEL <= $rank {
            $crate::console::print($crate::log::format_log!(
                $level,
                // 这里使用 format_args! 保留用户传来的格式字符串为字面量，以便在外层格式化时高亮显示
                format_args!($fmt $(, $($arg)+)?)
            ));
        }
    };
}

#[macro_export]
macro_rules! error {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::__log!($crate::log::LEVEL_ERROR, "ERROR", $fmt $(, $($arg)+)?);
    };
}

#[macro_export]
macro_rules! warn {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::__log!($crate::log::LEVEL_WARN, "WARN", $fmt $(, $($arg)+)?);
    };
}

#[macro_export]
macro_rules! info {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::__log!($crate::log::LEVEL_INFO, "INFO", $fmt $(, $($arg)+)?);
    };
}

#[macro_export]
macro_rules! debug {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::__log!($crate::log::LEVEL_DEBUG, "DEBUG", $fmt $(, $($arg)+)?);
    };
}

#[macro_export]
macro_rules! trace {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::__log!($crate::log::LEVEL_TRACE, "TRACE", $fmt $(, $($arg)+)?);
    };
}
