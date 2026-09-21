# 输出目录
O ?= build

# 工作目录
# 不要修改该变量，应与 .cargo/config.toml 中的 target-dir 保持一致
WRKDIR := target

# 目标平台：通常由顶层 Makefile 定义并导出；单独在本目录构建时回退到默认值
TARGET_NAME ?= riscv64gc-unknown-none-elf-jh7110

TARGET_DIR := $(WRKDIR)/$(TARGET_NAME)/release

# 用户程序源码目录
U_SRCDIR := src/bin

U_SRCS := $(wildcard $(U_SRCDIR)/*.rs)

# 用户程序 ELF 路径
ELFS := $(patsubst $(U_SRCDIR)/%.rs, $(TARGET_DIR)/%, $(U_SRCS))

# 当前源码对应的期望产物集合，用于反查 $(O) 中的过期 bin（源被删除/改名后遗留）
U_ELFS := $(patsubst $(U_SRCDIR)/%.rs, $(O)/%, $(U_SRCS))

all: bin

.PHONY: all clean FORCE elf

FORCE:

# 依赖判断全部交由 cargo 完成：
#   - .rs/.S/Cargo.toml/features 等由 cargo 增量机制原生追踪；
# make 无需也不应复刻这套逻辑，故用 FORCE 保证 cargo 每次都被调用；
# cargo 判定无需重编时不会重写 ELF（mtime 不变）。
elf: FORCE
	cargo build --release

# elf 作为前置保证 cargo 先执行；随后把 ELF 拷到输出目录。
# 内核侧用 MemorySet::from_elf 加载，故这里必须是 ELF，不能 objcopy 成扁平二进制。
# 按内容决定是否拷贝：内容未变时不动 mtime，避免误触发内核的 rerun-if-changed。
bin: elf
	mkdir -p $(O)
	@$(foreach e,$(ELFS),out=$(patsubst $(TARGET_DIR)/%,$(O)/%,$(e)); cmp -s "$(e)" "$$out" || cp "$(e)" "$$out";)
	@stale='$(filter-out $(U_ELFS),$(wildcard $(O)/*))'; \
	if [ -n "$$stale" ]; then \
		echo "  清理过期用户程序产物:"; \
		for f in $$stale; do echo "    RM $$f"; done; \
		rm -f $$stale; \
	fi

clean:
	rm -rf $(WRKDIR)

cargo-check:
	cargo check --target-dir $(WRKDIR) --release
	cargo clippy
