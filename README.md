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
    - Rust 1.99.0-nightly (c4af71034 2026-07-06)
- Python 3.12

## 构建&上板测试

```bash
# 构建 kernel.bin.img
make
# 上板测试
python3 tools/scripts/full_flash_test.py --boot_mode=2
```

## License

本项目遵循GPLv3开源协议，详见 [LICENSE](LICENSE) 文件。