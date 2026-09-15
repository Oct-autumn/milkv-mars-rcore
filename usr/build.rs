use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// 内核配置文件的相对路径（用户程序基址/大小/数量以 kernel/config.toml 为单一事实来源）。
const KERNEL_CONFIG_REL: &str = "../kernel/config.toml";

/// 解析 config.toml 中形如 `key = value` 的整型配置项。
/// 返回 (key, 十进制或十六进制整数值) 列表；# 开头的行为注释。
/// 解析规则与 kernel/build.rs 保持一致。
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

/// 获取 usr 目录下的用户程序列表，按文件名前缀的数字排序。
fn get_usr_apps(path: &Path, max_app_num: usize) -> Result<Vec<PathBuf>, String> {
    let mut apps = fs::read_dir(path)
        .map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?
        .filter_map(|f_res| {
            if let Ok(f) = f_res
                && let Ok(f_name) = f.file_name().into_string()
                && f_name.ends_with(".rs")
            {
                return Some(f.path());
            }

            None
        })
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

/// 生成应用程序的链接脚本
fn gen_linker_script_for_app(
    app_index: usize,
    app_base_address: usize,
    max_app_size: usize,
    out_dir: &Path,
) -> Result<PathBuf, String> {
    let linker_script_path = out_dir.join(format!("linker_{}.lds", app_index));

    let linker_script_content = format!(
        r#"/* 由 build.rs 自动生成，请勿手动修改 */
OUTPUT_ARCH(riscv)
ENTRY(_start)

BASE_ADDRESS = {:#x};

SECTIONS
{{
    . = BASE_ADDRESS;
    .text : {{
        *(.text.entry)
        *(.text .text.*)
    }}
    .rodata : {{
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }}
    .data : {{
        *(.data .data.*)
        *(.sdata .sdata.*)
    }}
    .bss : {{
        start_bss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        end_bss = .;
    }}
    /DISCARD/ : {{
        *(.eh_frame)
        *(.debug*)
    }}
}}"#,
        app_base_address + app_index * max_app_size
    );

    fs::write(&linker_script_path, linker_script_content)
        .map_err(|e| format!("写入 {} 失败: {}", linker_script_path.display(), e))?;

    Ok(linker_script_path)
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR 缺失");
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR 缺失");

    // ---- 配置变更追踪 ----
    // 读取 kernel/config.toml 后，必须显式声明依赖，否则配置变化不会重跑本脚本。
    // 注意：一旦声明了 rerun-if-changed，cargo 就不再默认监听整个包，
    // 因此还要显式盯住 src/bin 目录，才能在新增/删除用户程序时重新注入链接参数。
    println!("cargo:rerun-if-changed={KERNEL_CONFIG_REL}");
    println!("cargo:rerun-if-changed=src/bin");

    // ---- 从 kernel/config.toml 读取共享配置（单一事实来源）----
    let cfg_path = Path::new(&manifest_dir).join(KERNEL_CONFIG_REL);
    let configs = match parse_int_configs(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("build.rs 错误: {e}");
            std::process::exit(1);
        }
    };
    let get = |key: &str| configs.iter().find(|(k, _)| k == key).map(|(_, v)| *v);

    let app_base_address = get("app_base_address").unwrap_or(0x4040_0000) as usize;
    let max_app_size = get("app_max_size").unwrap_or(0x200_000) as usize;
    let max_app_num = get("max_app_num").unwrap_or(16) as usize;

    let usr_prog_dir = "src/bin";
    let usr_prog_path = Path::new(&manifest_dir).join(usr_prog_dir);

    // 扫描 usr 目录下的所有应用程序
    let apps = match get_usr_apps(&usr_prog_path, max_app_num) {
        Ok(apps) => apps,
        Err(e) => {
            eprintln!("build.rs 错误: {e}");
            std::process::exit(1);
        }
    };

    // 给每个应用程序单独生成一个链接脚本，并配置 cargo:rustc-link-arg-bin 来链接它们
    let linker_path = Path::new(&out_dir).join("linkers");
    // 确保输出目录存在
    fs::create_dir_all(&linker_path).expect("创建linker输出目录失败");
    for (i, app_path) in apps.iter().enumerate() {
        let linker_script_path =
            match gen_linker_script_for_app(i, app_base_address, max_app_size, &linker_path) {
                Ok(path) => path,
                Err(e) => {
                    eprintln!("build.rs 错误: {e}");
                    std::process::exit(1);
                }
            };
        println!(
            "cargo:rustc-link-arg-bin={}=-T{}",
            app_path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or_else(|| panic!("{} 不是合法的文件名", app_path.display())),
            linker_script_path.display()
        );
    }
}
