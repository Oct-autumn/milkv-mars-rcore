use crate::sbi_call;

use core::fmt::{self, Write};

#[allow(unused)]
struct Stdout;

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            sbi_call::console_putbyte(b);
        }
        Ok(())
    }
}

#[allow(unused)]
pub fn print(args: fmt::Arguments) {
    Stdout.write_fmt(args).unwrap();
}

#[allow(unused)]
pub fn write_bytes(bytes: &[u8]) {
    for &b in bytes {
        sbi_call::console_putbyte(b);
    }
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
