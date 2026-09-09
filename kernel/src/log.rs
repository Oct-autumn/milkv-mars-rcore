// 四级Log macro：Error > Warn > Info > Debug
// 通过 time::get_time() 获取时间（微秒），换算成秒（保留小数点后三位）

macro_rules! format_log {
    ($level: literal, $inner: expr) => {
        format_args!(
            "{}[ {: >.3}] | {: >5} | {}\x1b[0m\n",
            match $level {
                "ERROR" => "\x1b[1;31m",
                "WARN" => "\x1b[1;33m",
                "INFO" => "\x1b[1;32m",
                "DEBUG" => "\x1b[1;36m",
                _ => "\x1b[0m",
            },
            ($crate::time::get_time() as f64) / 1_000_000.0,
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
