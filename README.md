# Mars rCore (rCore-Tutorial-Book-v4)

本仓库是基于 `Milk-V Mars` 开发板的 rCore 操作系统教程的代码仓库兼课本，目前仍在施工中🚧

> - 本仓库参考了 [rCore-Tutorial-Book-v3](https://github.com/rcore-os/rCore-Tutorial-Book-v3)
> - 本仓库使用了 OpenCode 进行 Agent 辅助编码。

## 开发板介绍

Milk-V Mars 是一款基于 JH7110 处理器的 RISC-V 开发板，具有丰富的外设接口和良好的扩展性，适合操作系统开发和嵌入式系统学习。

主要板载资源有：
- 处理器：JH7110
    - 主核心：4*U74@1GHz（最高可达1.5GHz），支持RV64GC指令集，支持硬件浮点运算；
    - 监控核心：S7@1GHz，支持RV64IMAC
- 内存：2/4/8GB LPDDR4
- 存储：SPI Flash、SD Card（选配）、eMMC（选配）
- 外设接口：USB、UART、SPI、I2C、GPIO、PWM、PCIe 2.0、千兆以太网口等

## 开发环境

要搭建开发环境，请参考 [开发环境搭建](docs/dev-env.md)。此处只列出关键依赖：

- Rust 工具链
    - Rust 1.100.0-nightly (215a8af4b 2026-09-15)，且 LLVM 需 ≥ 23
        - 必须为 nightly：构建依赖 `build-std` 与自定义 target spec（`json-target-spec`）两个 unstable 特性
        - 版本下限来自 [rust-lang/rust#80608](https://github.com/rust-lang/rust/issues/80608) 的上游修复（rustc PR #160594，需 LLVM ≥ 23）：低于该版本时，`trap.S` 中的浮点汇编会被误报为 `instruction requires 'D'`
- Python 3.12（上板脚本依赖 `pyserial`；XMODEM/YMODEM 协议由脚本自带实现，**无需安装 `lrzsz`**）

## 构建&上板测试

开发板无 QEMU，通过串口烧录：BootROM 用 XMODEM 收 SPL，SPL 再用 YMODEM 收 SBI / kernel 镜像。
协议收发由 [`tools/scripts/full_flash_test.py`](tools/scripts/full_flash_test.py) 自行实现，传输过程实时显示进度条（百分比 / 字节数 / 速率 / 预计剩余时间 / 重传次数），全程字节流记录在 `--trace` 指定的日志中。

```bash
# 构建 kernel.bin.img
make

# 上板测试（默认 /dev/ttyACM0@115200）
python3 tools/scripts/full_flash_test.py --boot_mode=1   # SBI + TBT
python3 tools/scripts/full_flash_test.py --boot_mode=2   # 仅 TBT（SBI 从 flash 读）

# 常用可选项
#   --port /dev/ttyUSB0   指定串口      --baud 115200
#   --verbose             打印邀请/块0/重传等协议细节，便于排查上板问题
#   --ymodem_block 128    YMODEM 退回 128B 小包
#   --auto_script x.py    引导完成后执行自动化脚本（注入 ctx：write_line/expect/wait）
#   --trace /tmp/full_trace.bin        完整字节流日志路径（默认值）
```

## License

本项目遵循GPLv3开源协议，详见 [LICENSE](LICENSE) 文件。