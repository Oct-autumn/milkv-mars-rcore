# 输出目录
O ?= build

# 工作目录
# 不要修改该变量，应与 .cargo/config.toml 中的 target-dir 保持一致
WRKDIR := target

TARGET_DIR := $(WRKDIR)/riscv64gc-unknown-none-elf/release

# 用户程序源码目录
U_SRCDIR := src/bin

U_SRCS := $(wildcard $(U_SRCDIR)/*.rs)

# 用户程序 ELF 路径
ELFS := $(patsubst $(U_SRCDIR)/%.rs, $(TARGET_DIR)/%, $(U_SRCS))

all: bin

.PHONY: all clean FORCE elf

FORCE:

# 依赖判断全部交由 cargo 完成：
#   - .rs/.S/Cargo.toml/features 等由 cargo 增量机制原生追踪；
#   - 各用户程序的链接脚本由 build.rs 逐个生成并注入，app 集合变化后 cargo 会重链。
# make 无需也不应复刻这套逻辑，故用 FORCE 保证 cargo 每次都被调用；
# cargo 判定无需重编时不会重写 ELF（mtime 不变）。
elf: FORCE
	cargo build --release

# elf 作为前置保证 cargo 先执行；随后按 mtime 判断各 BIN 是否需要重新 objcopy。
# 关键：ELF 未变时跳过 objcopy，避免刷新 .bin 的时间戳——否则会误触发
# 内核侧的 rerun-if-changed，导致内核在无改动时也被重编。
bin: elf
	mkdir -p $(O)
	@$(foreach e,$(ELFS),out=$(patsubst $(TARGET_DIR)/%,$(O)/%.bin,$(e)); if [ ! -e "$$out" ] || [ "$(e)" -nt "$$out" ]; then rust-objcopy --strip-all "$(e)" -O binary "$$out"; fi;)

clean:
	rm -rf $(WRKDIR)

cargo-check:
	cargo check --target-dir $(WRKDIR) --release
