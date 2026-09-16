#![no_std]
#![no_main]

#[macro_use]
extern crate usr_lib;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // 计算 e 的近似值，使用级数展开：
    // e = 1 + 1/1! + 1/2! + 1/3! + 1/4! + ...
    // 迭代1_000_000次，每50_000次打印一次进度
    let iter = 1000000;
    let mut e = 1.0;
    let mut factorial = 1.0;
    for i in 1..=iter {
        factorial *= i as f64;
        e += 1.0 / factorial;
        if i % 50000 == 0 {
            println!("e [{}/{}]", i, iter);
        }
    }
    println!("e ≈ {}", e);
    0
}
