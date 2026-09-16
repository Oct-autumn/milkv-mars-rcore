#![no_std]
#![no_main]

#[macro_use]
extern crate usr_lib;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // 计算 π 的近似值，使用莱布尼茨级数展开：
    // π = 4 * (1 - 1/3 + 1/5 - 1/7 + 1/9 - 1/11 + ...)
    // 迭代1_000_000次，每50_000次打印一次进度
    let iter = 1000000;
    let mut pi = 0.0;
    for i in 1..=iter {
        let term = if i % 2 == 1 { 1.0 } else { -1.0 } / (2 * i - 1) as f64;
        pi += term;
        if i % 50000 == 0 {
            println!("π [{}/{}]", i, iter);
        }
    }
    pi *= 4.0;
    println!("π ≈ {}", pi);
    0
}
