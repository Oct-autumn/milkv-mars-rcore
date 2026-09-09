//! 构建脚本：将 kernel/config.toml 中的配置分发到链接脚本与 Rust 源码。
//!
//! 本脚本承担三类工作：
//!   1. 读取 config.toml，把数值配置（如 base_address）通过
//!      `-defsym` 注入链接脚本（rust-lld 的"宏"机制）；
//!   2. 生成 OUT_DIR/generated.rs，让 Rust 源码共享同一份配置常量；
//!   3. 追踪 config.toml 与 linker.lds 的变更，使 cargo 能够感知并触发重链。
//!
//! 板卡等"开关类"配置使用 Cargo features（见 Cargo.toml），
//! 在 build.rs 中可通过 CARGO_FEATURE_<NAME> 环境变量感知。

use std::env;
use std::fs;
use std::path::Path;

/// 简单的 FNV-1a 哈希，用于对 linker.lds 内容做摘要。
fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 解析 config.toml 中形如 `key = value` 的整型配置项。
/// 返回 (key, 十进制或十六进制整数值) 列表；# 开头的行为注释。
fn parse_int_configs(path: &Path) -> Result<Vec<(String, u64)>, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?;
    let mut out = Vec::new();
    for (lineno, raw) in content.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            return Err(format!(
                "{}:{} 不是合法的 key = value 配置",
                path.display(),
                lineno + 1
            ));
        };
        let key = key.trim();
        let val = val.trim();
        let v = if let Some(hex) = val.strip_prefix("0x").or_else(|| val.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16)
                .map_err(|_| format!("{}:{} 十六进制值非法", path.display(), lineno + 1))?
        } else {
            val.parse::<u64>()
                .map_err(|_| format!("{}:{} 整数值非法", path.display(), lineno + 1))?
        };
        out.push((key.to_string(), v));
    }
    Ok(out)
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR 缺失");
    let cfg_path = Path::new(&manifest_dir).join("config.toml");
    let lds_path = Path::new(&manifest_dir).join("src/linker.lds");

    // ---- 配置变更追踪 ----
    // 注意：rerun-if-changed 的路径必须是相对 CARGO_MANIFEST_DIR 的路径，
    // 传绝对路径不会生效（cargo 无法与之匹配）。
    println!("cargo:rerun-if-changed=config.toml");
    println!("cargo:rerun-if-changed=src/linker.lds");

    // ---- 读取 config.toml ----
    let configs = match parse_int_configs(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("build.rs 错误: {e}");
            std::process::exit(1);
        }
    };
    let get = |key: &str| configs.iter().find(|(k, _)| k == key).map(|(_, v)| *v);

    let base_address = get("base_address").unwrap_or(0x4020_0000);
    let mtime_frequency = get("mtime_frequency").unwrap_or(4_000_000);

    // ---- 分发 1: 注入链接脚本宏（rust-lld 的 -defsym 机制）----
    println!("cargo:rustc-link-arg=-defsym=BASE_ADDRESS=0x{base_address:x}");

    // ---- 分发 2: 生成 Rust 常量（Rust 侧与链接脚本共享同一事实来源）----
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR 缺失");
    let generated = Path::new(&out_dir).join("generated.rs");
    fs::write(
        &generated,
        format!(
            "// 由 build.rs 从 config.toml 自动生成，请勿手动修改\n\
         pub const BASE_ADDRESS: usize = 0x{base_address:x};\n
         pub const MTIME_FREQUENCY: usize = {mtime_frequency};\n",
        ),
    )
    .expect("写入 generated.rs 失败");

    // ---- 分发 3: 让 cargo 感知 linker.lds 的内容变更 ----
    // 链接脚本通过 -Clink-arg=-T... 传给链接器，cargo 的 dep-info 不会追踪它；
    // 这里把其内容摘要注入一个无害链接符号，内容一变 → 链接参数变 → cargo 自动重链。
    if let Ok(lds_content) = fs::read(&lds_path) {
        let hash = fnv1a(&lds_content);
        println!("cargo:rustc-link-arg=-defsym=LDS_CONTENT_HASH=0x{hash:016x}");
    }
}
