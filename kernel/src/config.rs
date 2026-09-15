//! 内核编译期配置。
//!
//! 两类配置在此汇合：
//!   1. 开关类（板卡选择等）——由 Cargo features 控制，见 Cargo.toml；
//!   2. 数值/字符串类（BASE_ADDRESS、LOG_LEVEL_NAME 等）——由 build.rs 从 config.toml 生成，见下方 include!。

// ---- 开关配置（Cargo features，在 Cargo.toml 中定义）----
//#[cfg(feature = "board-qemu")]
//pub const BOARD_QEMU: bool = true;

// ---- 数值配置（build.rs 从 config.toml 自动生成，单一事实来源）----
include!(concat!(env!("OUT_DIR"), "/generated.rs"));
