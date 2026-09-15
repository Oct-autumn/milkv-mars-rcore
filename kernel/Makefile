# 输出目录
O ?= build

# 工作目录
# 不要修改该变量，应与 .cargo/config.toml 中的 target-dir 保持一致
WRKDIR := target

# 用户程序产物目录，供 build.rs 读取（顶层 Makefile 会以同名环境变量覆盖）
USR_BIN_DIR ?= $(abspath ../build/usr)
export USR_BIN_DIR

# 内核 ELF 文件路径
K_ELF := $(WRKDIR)/riscv64gc-unknown-none-elf/release/kernel
# 内核 BIN 文件路径
K_BIN := $(O)/kernel.bin

all: $(K_BIN)

.PHONY: all clean FORCE

# 依赖判断全部交由 cargo 完成：
#   - .rs/.S/Cargo.toml/features 等由 cargo 增量机制原生追踪；
#   - config.toml 与 linker.lds 由 build.rs 追踪并触发重链。
# make 无需也不应复刻这套逻辑，故用 FORCE 保证 cargo 每次都被调用；
# cargo 判定无需重编时不会重写 ELF（mtime 不变），
# 下游 objcopy 步骤便会因"目标不比依赖新"而被 make 自动跳过。
FORCE:

# 使用 cargo 构建内核 ELF 文件
$(K_ELF): FORCE
	cargo build --target-dir $(WRKDIR) --release

# 使用 rust-objcopy 将 ELF 文件转换为 BIN 文件
$(K_BIN): $(K_ELF)
	mkdir -p $(dir $@)
	rust-objcopy --strip-all $< -O binary $@

clean:
	rm -rf $(WRKDIR)

cargo-check:
	cargo check --target-dir $(WRKDIR) --release
