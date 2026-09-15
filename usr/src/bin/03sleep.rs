#![no_std]
#![no_main]

#[macro_use]
extern crate usr_lib;

use usr_lib::process::{TimeVal, sys_get_time, sys_yield};

/// 目标睡眠时长（微秒）
const SLEEP_US: usize = 5_000_000;
/// 轮询次数上界，防止时钟异常时空转
const MAX_POLLS: usize = 10_000_000;

/// 读取当前系统时间（微秒）；sys_get_time 失败时返回 None
fn now_us() -> Option<usize> {
    let mut ts = TimeVal { sec: 0, usec: 0 };
    if sys_get_time(&mut ts, 0) != 0 {
        return None;
    }
    Some(ts.sec * 1_000_000 + ts.usec)
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    let Some(start) = now_us() else {
        println!("Test sleep FAILED: sys_get_time error");
        return -1;
    };
    let deadline = start + SLEEP_US;

    for _ in 0..MAX_POLLS {
        match now_us() {
            Some(now) if now >= deadline => {
                println!("Test sleep OK!");
                return 0;
            }
            Some(_) => {
                sys_yield();
            }
            None => {
                println!("Test sleep FAILED: sys_get_time error");
                return -1;
            }
        }
    }

    println!("Test sleep FAILED: poll limit exceeded");
    -1
}
