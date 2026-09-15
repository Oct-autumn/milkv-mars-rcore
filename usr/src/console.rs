use crate::sys_call::fs::sys_write;

use core::fmt::{self, Write};

#[allow(unused)]
struct Stdout;

const STDOUT: usize = 1;

// TODO(下一阶段-行缓冲)：core::fmt 会把一次格式化输出拆成多次 write_str，每次
// write_str 都是一次独立的 sys_write；抢占式调度可能在两次 sys_write 之间切走任务，
// 导致多任务输出在同一行内交错。待实现定长行缓冲（[u8; N]，无需堆）后，攒满一行
// （或缓冲满）再一次性提交，保证单行输出不被抢占打断。
impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        sys_write(STDOUT, s.as_bytes());
        Ok(())
    }
}

#[allow(unused)]
pub fn print(args: fmt::Arguments) {
    Stdout.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! print {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(format_args!($fmt $(, $($arg)+)?));
    }
}

#[macro_export]
macro_rules! println {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(format_args!("{}\n", format_args!($fmt $(, $($arg)+)?)));
    };
}
