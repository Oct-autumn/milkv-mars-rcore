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

# 当前源码对应的期望产物集合，用于反查 $(O) 中的过期 bin（源被删除/改名后遗留）
U_BINS := $(patsubst $(U_SRCDIR)/%.rs, $(O)/%.bin, $(U_SRCS))

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
# $(wildcard) 必须写在 recipe 里：make 在执行本规则时才展开，取到的才是磁盘实时产物；
# 若提取为顶层 := 变量会在解析阶段求值，无法感知本次构建前的文件状态。
	@stale='$(filter-out $(U_BINS),$(wildcard $(O)/*.bin))'; \
	if [ -n "$$stale" ]; then \
		echo "  清理过期用户程序产物:"; \
		for f in $$stale; do echo "    RM $$f"; done; \
		rm -f $$stale; \
	fi

clean:
	rm -rf $(WRKDIR)

cargo-check:
	cargo check --target-dir $(WRKDIR) --release
