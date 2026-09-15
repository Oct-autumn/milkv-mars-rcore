# 最终输出目录
O := output
# 将最终输出目录设置为绝对路径，避免在不同目录下 执行make / 传参 时出现路径问题
O := $(abspath output)

# 工作目录
WRKDIR := build
# 将工作目录设置为绝对路径，避免在不同目录下 执行make / 传参 时出现路径问题
WRKDIR := $(abspath build)

# 导出用户程序产物目录，供 kernel/build.rs 读取
export USR_BIN_DIR := $(WRKDIR)/usr

# Kernel BIN 文件路径
K_BIN := $(WRKDIR)/kernel.bin

# 最终产物 - 内核 IMG 镜像文件
K_IMG := $(O)/kernel.bin.img

all: kernel

.PHONY: all clean FORCE

# FORCE 保证每次 make 都会进入子 Makefile 做一次"增量判断"，
# 是否真正重建由 kernel 子工程中的 cargo 决定。
# 无真实变更时子工程不会重写 kernel.bin（mtime 不变），
# 因此依赖 kernel.bin 的打包步骤同样会被 make 自动跳过。
FORCE:

# 调内部Makefile构建内核 BIN 文件
$(K_BIN): FORCE uprog-bin
	$(MAKE) -C kernel O=$(WRKDIR)

# 使用 tools/scripts/pack_image.py 将 BIN 文件打包为 IMG 镜像文件
$(K_IMG): $(K_BIN)
	mkdir -p $(dir $@)
	python3 tools/scripts/pack_image.py --tbt $< --output $(dir $@)

kernel: $(K_IMG)

uprog-bin: FORCE
	$(MAKE) -C usr O=$(WRKDIR)/usr

clean:
	rm -rf $(O) $(WRKDIR)
	$(MAKE) -C kernel O=$(WRKDIR) clean
	$(MAKE) -C usr O=$(WRKDIR) clean
