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
use std::path::PathBuf;

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
        // 带引号的是字符串配置（如 log_level），不属于整数配置，跳过
        if val.starts_with('"') {
            continue;
        }
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

/// 解析并校验 config.toml 中的 `log_level` 字符串配置。
///
/// 该键控制日志等级过滤，取值范围限定为 error|warn|info|debug|trace；
/// 未配置时回退到 "debug"（与加入过滤前的可见输出保持一致）。非法值直接报错。
fn parse_log_level(path: &Path) -> Result<String, String> {
    const VALID: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

    let content =
        fs::read_to_string(path).map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?;
    for (lineno, raw) in content.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "log_level" {
            continue;
        }
        let val = val.trim();
        let Some(v) = val
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
        else {
            return Err(format!(
                "{}:{} log_level 的值必须用双引号括起来，例如 log_level = \"debug\"",
                path.display(),
                lineno + 1
            ));
        };
        if !VALID.contains(&v) {
            return Err(format!(
                "{}:{} log_level = \"{}\" 非法，可选：error | warn | info | debug | trace",
                path.display(),
                lineno + 1,
                v
            ));
        }
        return Ok(v.to_string());
    }
    Ok("debug".to_string())
}

/// 获取用户程序产物目录（默认 ../build/usr）下的 `.bin` 列表，按文件名前缀的数字排序。
fn get_usr_apps(path: &Path, max_app_num: usize) -> Result<Vec<PathBuf>, String> {
    let mut apps = fs::read_dir(path)
        .map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?
        .filter_map(|f_res| f_res.ok().map(|f| f.path()))
        // 只认 .bin，避免把目录或编辑器临时文件误当成用户程序
        .filter(|p| p.extension().is_some_and(|ext| ext == "bin"))
        .collect::<Vec<_>>();

    // 将apps按文件名前缀的数字排序
    apps.sort_by_key(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .and_then(|s| {
                // 提取文件名前缀的数字部分
                let num_str = s
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>();
                num_str.parse::<usize>().ok()
            })
            .unwrap_or(usize::MAX)
    });

    let app_nums = apps.len();
    if app_nums < 1 {
        return Err("未找到任何用户程序".to_string());
    } else if app_nums > max_app_num {
        return Err(format!(
            "用户程序数量过多（{}），请确保不超过 {} 个",
            app_nums, max_app_num
        ));
    }

    Ok(apps)
}

/// 生成 usr_linker.S，将用户程序链入内核。
fn generate_usr_link_asm(apps: &[PathBuf], linker: &Path) -> Result<(), String> {
    // 生成 _num_app 符号
    let app_nums = apps.len();

    fs::write(
        linker,
        format!(
            r#"/* 由 build.rs 自动生成，请勿手动修改 */
    .align 3
    .section .data
    .global _num_app
_num_app:
    .quad {}
{}
    .quad _app_{}_end
{}
         "#,
            app_nums,
            apps.iter()
                .enumerate()
                .map(|(i, _)| { format!("    .quad _app_{i}_start") })
                .collect::<Vec<_>>()
                .join("\n"),
            app_nums - 1,
            apps.iter()
                .enumerate()
                .map(|(i, p)| {
                    let path = p.to_string_lossy();
                    format!(
                        r#"
    .section .data
    .global _app_{i}_start
    .global _app_{i}_end
_app_{i}_start:
    .incbin "{path}"
_app_{i}_end:"#
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    )
    .map_err(|e| format!("写入 {} 失败: {}", linker.display(), e))?;
    Ok(())
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR 缺失");
    let cfg_file = "config.toml";
    let lds_file = "src/linker.lds";
    // 用户程序产物目录：由 Makefile 通过 USR_BIN_DIR 注入，未设置时回退到 ../build/usr
    let usr_prog_dir = env::var("USR_BIN_DIR").unwrap_or_else(|_| "../build/usr".to_string());

    let cfg_path = Path::new(&manifest_dir).join(cfg_file);
    let lds_path = Path::new(&manifest_dir).join(lds_file);
    let usr_prog_path = Path::new(&manifest_dir).join(&usr_prog_dir);
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR 缺失");

    // ---- 配置变更追踪 (1) ----
    println!("cargo:rerun-if-changed={}", cfg_file);
    println!("cargo:rerun-if-changed={}", lds_file);
    // 目录级追踪：逐文件追踪无法感知“新增/删除”用户程序，必须额外盯住目录本身
    println!("cargo:rerun-if-changed={}", usr_prog_dir);
    println!("cargo:rerun-if-env-changed=USR_BIN_DIR");

    // 目录缺失时给出可操作的提示（干净检出/未先构建 usr 时会出现）
    if !usr_prog_path.is_dir() {
        eprintln!(
            "build.rs 错误: 未找到用户程序目录 {}。\n\
             请先在仓库根目录执行 `make`（或 `make uprog-bin`）以生成用户程序。",
            usr_prog_path.display()
        );
        std::process::exit(1);
    }

    // ---- 读取 log_level（字符串配置，缺省 debug）----
    // 先于整数配置解析：这样未加引号的写法能命中更明确的报错提示
    let log_level = match parse_log_level(&cfg_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("build.rs 错误: {e}");
            std::process::exit(1);
        }
    };

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
    let app_base_address = get("app_base_address").unwrap_or(0x4040_0000);

    // ---- 分发 1: 注入链接脚本宏（rust-lld 的 -defsym 机制）----
    println!("cargo:rustc-link-arg=-defsym=BASE_ADDRESS=0x{base_address:x}");
    // 供 linker.lds 中的 ASSERT 使用，确保内核镜像不会压到应用区
    println!("cargo:rustc-link-arg=-defsym=APP_BASE_ADDRESS=0x{app_base_address:x}");

    // ---- 分发 2: 生成 Rust 常量（Rust 侧与链接脚本共享同一事实来源）----
    let generated = Path::new(&out_dir).join("generated.rs");
    fs::write(
        &generated,
        format!(
            r#"// 由 build.rs 从 config.toml 自动生成，请勿手动修改
pub const BASE_ADDRESS: usize = 0x{:x};
pub const MTIME_FREQUENCY: usize = {};
pub const USER_STACK_SIZE: usize = {};
pub const KERNEL_STACK_SIZE: usize = {};
pub const MAX_APP_NUM: usize = {};
pub const APP_BASE_ADDRESS: usize = 0x{:x};
pub const APP_SIZE_LIMIT: usize = 0x{:x};
pub const LOG_LEVEL_NAME: &str = "{}";
"#,
            base_address,
            get("mtime_frequency").unwrap_or(4_000_000),
            get("user_stack_size").unwrap_or(4096 * 2),
            get("kernel_stack_size").unwrap_or(4096 * 2),
            get("max_app_num").unwrap_or(16),
            app_base_address,
            get("app_max_size").unwrap_or(0x200_000),
            log_level
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

    // ---- 分发 4: 生成 usr_linker.S，把用户程序链入内核 ----
    let usr_apps = match get_usr_apps(&usr_prog_path, get("max_app_num").unwrap_or(16) as usize) {
        Ok(apps) => apps,
        Err(e) => {
            eprintln!("build.rs 错误: {e}");
            std::process::exit(1);
        }
    };
    // ---- 追踪用户程序变更 (2) ----
    for app in &usr_apps {
        // 追踪用户程序变更
        println!("cargo:rerun-if-changed={}", app.display());
    }
    let generated_usr_linker = Path::new(&out_dir).join("usr_linker.S");
    if let Err(e) = generate_usr_link_asm(&usr_apps, &generated_usr_linker) {
        eprintln!("build.rs 错误: {e}");
        std::process::exit(1);
    }
}
