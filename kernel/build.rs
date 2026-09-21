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

use std::{env, fs, path::Path, path::PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    k_base_address: usize,
    mtime_frequency: usize,
    u_stack_size: usize,
    k_stack_size: usize,
    stimer_interval: usize,
    k_heap_size_shift: usize,
    page_size_shift: usize,
    mem_base_address: usize,
    log_level: String,
}

/// 简单的 FNV-1a 哈希，用于对 linker.lds 内容做摘要。
fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn gen_config_constants(configs: &Config, out_path: &Path) {
    let mut content = Vec::new();

    content.push(String::from(
        "// 由 build.rs 从 config.toml 自动生成，请勿手动修改",
    ));
    content.push(format!(
        r#"/// 内存基地址
pub const MEM_BASE_ADDRESS: usize = 0x{:x};"#,
        configs.mem_base_address
    ));
    content.push(String::from(
        r#"/// 内存大小（字节，默认 1GB）
/// 说明：JH7110支持2/4/8GB三种内存规格，实际内存大小由EEPROM里的SerialID决定，
/// 或通过设备树文件的memory节点指定。此处我们先不动态设置内存大小，
/// 而是在编译期固定为1GB，后续可考虑通过设备树文件动态设置。
pub const MEM_SIZE: usize = 0x4000_0000; // 1GB
/// 内存结束地址（字节）
pub const MEM_END_ADDR: usize = MEM_BASE_ADDRESS + MEM_SIZE;
/// 跳板页的虚拟地址
pub const TRAMPOLINE: usize = usize::MAX - PAGE_SIZE + 1;
/// 中断上下文的虚拟地址
pub const TRAP_CONTEXT: usize = TRAMPOLINE - PAGE_SIZE;"#,
    ));
    content.push(format!(
        r#"/// 内核基地址
pub const K_BASE_ADDRESS: usize = 0x{:x};"#,
        configs.k_base_address
    ));
    content.push(format!(
        r#"/// 时钟频率（Hz）
pub const MTIME_FREQUENCY: usize = {};"#,
        configs.mtime_frequency
    ));
    content.push(format!(
        r#"/// 用户栈大小（字节）
pub const U_STACK_SIZE: usize = {};"#,
        configs.u_stack_size
    ));
    content.push(format!(
        r#"/// 内核栈大小（字节）
pub const K_STACK_SIZE: usize = {};
/// 内核栈虚拟基址（首个任务的栈顶 = TRAP_CONTEXT，后续任务依次向下排布）
/// 注意：必须引用 TRAP_CONTEXT（页对齐），不能写成 usize::MAX - PAGE_SIZE，
/// 后者会落在 TRAP_CONTEXT 页的页尾字节上，导致栈顶未对齐并侵占该页。
pub const K_STACK_V_BASE: usize = TRAP_CONTEXT;"#,
        {
            assert!(
                configs
                    .k_base_address
                    .is_multiple_of(1 << configs.page_size_shift)
            );
            configs.k_stack_size
        }
    ));
    content.push(format!(
        r#"/// 定时器中断间隔（单位：微秒）
pub const STIMER_INTERVAL: usize = {};"#,
        configs.stimer_interval
    ));
    content.push(format!(
        r#"/// 内核堆大小（位数）
pub const K_HEAP_SIZE_SHIFT: usize = {};
/// 内核堆大小（2^K_HEAP_SIZE_SHIFT 字节）
pub const K_HEAP_SIZE: usize = 1 << K_HEAP_SIZE_SHIFT;"#,
        configs.k_heap_size_shift
    ));
    content.push(format!(
        r#"/// 页大小（位数）
pub const PAGE_SIZE_SHIFT: usize = {};
/// 页大小（字节）
pub const PAGE_SIZE: usize = 1 << PAGE_SIZE_SHIFT;"#,
        configs.page_size_shift
    ));
    content.push(format!(
        r#"/// 日志等级
pub const LOG_LEVEL_NAME: &str = "{}";"#,
        {
            let loer_case_level = configs.log_level.to_lowercase();
            if ["error", "warn", "info", "debug", "trace"].contains(&loer_case_level.as_str()) {
                loer_case_level
            } else {
                "info".to_string()
            }
        }
    ));

    fs::write(out_path, content.join("\n")).expect("写入 generated.rs 失败");
}

/// 获取用户程序产物目录（默认 ../build/usr）下的elf文件列表，按文件名前缀的数字排序。
fn get_usr_apps(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut apps = fs::read_dir(path)
        .map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?
        .filter(|f_res| match f_res {
            // 只读取文件类型为普通文件的条目，避免目录/链接/设备文件等干扰
            Ok(f) => f.file_type().map(|ft| ft.is_file()).unwrap_or(false),
            Err(_) => false,
        })
        .filter_map(|f_res| f_res.ok().map(|f| f.path()))
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

    let toml_str = fs::read_to_string(&cfg_path)
        .unwrap_or_else(|e| panic!("读取 {} 失败: {}", cfg_path.display(), e));
    let configs: Config = toml::from_str(&toml_str).unwrap();

    // ---- 分发 1: 注入链接脚本宏（rust-lld 的 -defsym 机制）----
    println!(
        "cargo:rustc-link-arg=-defsym=BASE_ADDRESS=0x{:x}",
        configs.k_base_address
    );

    // ---- 分发 2: 生成 Rust 常量（Rust 侧与链接脚本共享同一事实来源）----
    let generated = Path::new(&out_dir).join("generated.rs");
    gen_config_constants(&configs, &generated);

    // ---- 分发 3: 让 cargo 感知 linker.lds 的内容变更 ----
    // 链接脚本通过 -Clink-arg=-T... 传给链接器，cargo 的 dep-info 不会追踪它；
    // 这里把其内容摘要注入一个无害链接符号，内容一变 → 链接参数变 → cargo 自动重链。
    if let Ok(lds_content) = fs::read(&lds_path) {
        let hash = fnv1a(&lds_content);
        println!("cargo:rustc-link-arg=-defsym=LDS_CONTENT_HASH=0x{hash:016x}");
    }

    // ---- 分发 4: 生成 usr_linker.S，把用户程序链入内核 ----
    let usr_apps = match get_usr_apps(&usr_prog_path) {
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
