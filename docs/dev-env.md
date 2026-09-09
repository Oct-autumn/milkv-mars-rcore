# 搭建开发环境

**推荐在Linux系统或WSL中搭建开发环境**

## 基础开发软件

```bash
sudo apt-get update && sudo apt-get upgrade
sudo apt-get install git build-essential
```

## Rust 开发环境搭建

### （可选）Rustup 换源

你需要在shell里添加两个环境变量`RUSTUP_DIST_SERVER`和`RUSTUP_UPDATE_ROOT`，以使用国内镜像源加速 Rustup 的下载：（此处使用阿里云镜像源作为示例，你也可以换成别的镜像源，比如`TUNA`）

```bash
# Bash
export RUSTUP_DIST_SERVER=https://mirrors.aliyun.com/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.aliyun.com/rustup/rustup
# Fish
set -x RUSTUP_DIST_SERVER https://mirrors.aliyun.com/rust-static
set -x RUSTUP_UPDATE_ROOT https://mirrors.aliyun.com/rustup/rustup
```

### 安装 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**注意：** 
- 安装时选择`nightly`工具链，因为我们的要使用一些nightly特性；
- 由于未使用`sudo`，请确保将`~/.cargo/bin`添加到`PATH`中，并重启终端使其生效；

### 检查 Rust 是否安装成功

检查rust是否正确安装：

```bash
rustc --version
```

你应该看到类似如下输出：

```
rustc 1.99.0-nightly (c4af71034 2026-07-06)
```

### （可选）Cargo 换源

修改或新建`~/.cargo/config.toml`文件，添加如下内容：（此处使用阿里云镜像源作为示例，你也可以换成别的镜像源，比如`TUNA`）

```toml
[source.crates-io]
replace-with = 'aliyun'

[source.aliyun]
registry = "sparse+https://mirrors.aliyun.com/crates.io-index/"
```

### 安装 Rust 相关的软件包

```bash
rustup target add riscv64gc-unknown-none-elf
cargo install cargo-binutils
rustup component add llvm-tools
rustup component add rust-src
```

## IDE配置

我推荐你使用 VSCode 作为 IDE，并安装以下插件：

- `Rust Analyzer`：提供Rust语言支持
- `RISC-V Support`：提供RISC-V汇编支持