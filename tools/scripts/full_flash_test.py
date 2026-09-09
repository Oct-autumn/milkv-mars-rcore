#!/usr/bin/env python3
"""
JH7110 上板测试脚本（Milk-V Mars）

====================================================================
运行步骤：
- 等待上电后 BootROM 稳定，累计收集足够多的 'C' 邀请
- 启动 sx 发送 SPL .normal.out 文件
- 等待 SPL 启动，跳过 Recovery 菜单，选择引导模式（1:SBI+TBT/2:TBT only）
- 等待 SPL 进入 ymodem 接收阶段
- 启动 sb 发送文件（根据选择的引导模式，发送不同的文件）
- 等待引导
- 接收后续串口输出，读取键盘输入反馈到串口
- 全程记录日志到 full_trace.bin
====================================================================

串口链路说明（基于 SPL 实测输出）：
- BootROM 上电后持续输出 '(C)StarFive' 与 'C' 邀请（XMODEM-CRC 握手），
  用 sx 上传 SPL。
- SPL 启动后打印 '==== Minimum SPL - v0.1 ===='，随后进入 Recovery
  倒计时（'Entering Recovery Menu in 05 seconds ... (Press any key to skip)'），
  任意键（回车）可跳过。
- 跳过 Recovery 后出现 '==== Select Boot Mode ====' 菜单：
    0: Boot From Flash
    1: Boot From UART (SBI + TBT)
    2: Boot From UART (TBT only)
  输入 1/2 并回车确认。
- 模式 1（SBI+TBT）：先 'Transfer SBI image via YMODEM now...'（sb 传 SBI），
  收到 'Done!' 后 SPL 自动输出 'Transfer TBT image via YMODEM now...'（sb 传 TBT）。
- 模式 2（TBT only）：'Reading SBI image from flash ...Done!' 后
  直接 'Transfer TBT image via YMODEM now...'（sb 传 TBT）。
- 传输成功标志：'Recv success, total size: ...Bytes.' + 'Done!'。
- 最后 'Booting SBI ...' 表示 SPL 已把控制权交给 SBI/三级引导。

传输阶段的串口数据由脚本独占，sx/sb 运行在伪终端(pty)里，脚本在
串口与 pty 之间双向桥接，因此 full_trace.bin 可以全程连续记录。
"""

import os
import sys
import time
import pty
import select
import serial
import subprocess
import re
import tty
import termios
import argparse

# ----------------------------------------------------------------------------
# 全局 abort：打印错误并以非零码退出（资源由 finally 统一关闭）
# ----------------------------------------------------------------------------
def abort(msg):
    print(f"\n[tool] {msg}", flush=True)
    raise SystemExit(1)


# ----------------------------------------------------------------------------
# 日志文件：只写 + 即时落盘
# ----------------------------------------------------------------------------
class Trace:
    def __init__(self, path):
        self.f = open(path, "wb")

    def write(self, data):
        if data:
            self.f.write(data)
            self.f.flush()

    def close(self):
        try:
            self.f.close()
        except Exception:
            pass


# ----------------------------------------------------------------------------
# Uart 会话：封装串口读取、日志、实时回显与跨阶段缓冲区
#
# 核心约定：所有从串口读到的数据都走 _ingest() ->
#   1) 写入日志 full_trace.bin（全程连续）
#   2) 追加进 self.buffer（供 read_until 跨阶段匹配，避免丢数据）
#   3) 若 self.tee 为真，实时回显到终端（传输阶段关闭，避免二进制乱码）
# ----------------------------------------------------------------------------
class Uart:
    def __init__(self, ser, trace, tee=True):
        self.ser = ser
        self.trace = trace
        self.tee = tee
        self.buffer = bytearray()

    def _ingest(self, data):
        """接收串口数据：写日志 + 入缓冲区 + 可选回显。返回字节数。"""
        if not data:
            return 0
        self.trace.write(data)
        self.buffer += data
        if self.tee:
            try:
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()
            except (BrokenPipeError, OSError):
                pass
        return len(data)

    def pump(self, timeout):
        """阻塞读取串口至多 timeout 秒，返回期间累计接收的字节数。"""
        end = time.time() + timeout
        total = 0
        while time.time() < end:
            r, _, _ = select.select([self.ser], [], [], 0.05)
            if r:
                data = self.ser.read(4096)
                if not data:
                    # 串口可读但读到空 => 连接异常/断开
                    raise serial.SerialException("串口读取到空数据，可能已断开")
                total += self._ingest(data)
            else:
                time.sleep(0.01)
        return total

    def read_until(self, patterns, timeout, label=None):
        """等待串口数据命中任一正则 pattern（bytes）。

        命中后返回 (pattern, consumed)，consumed 为从缓冲区开头到匹配结束
        的原始字节（匹配之前的遗留数据一并消费），匹配之后的数据保留在
        buffer 中供下一阶段使用。超时返回 (None, None)。
        """
        start = time.time()
        buf = bytes(self.buffer)
        while time.time() - start < timeout:
            for p in patterns:
                m = re.search(p, buf)
                if m:
                    consumed_end = m.end()
                    consumed = bytes(self.buffer[:consumed_end])
                    del self.buffer[:consumed_end]
                    if label:
                        print(f"\n[{label}] 命中: {p!r}", flush=True)
                    return p, consumed
            # 未命中则继续收取数据，并把新数据并入待匹配 buf
            n = self.pump(0.2)
            if n:
                buf = bytes(self.buffer)
        if label:
            print(f"\n[{label}] 等待超时 {timeout}s", flush=True)
        return None, None

    def send(self, data):
        self.ser.write(data)

    def send_line(self, s):
        """发送一行文本并回车（菜单确认、跳过菜单等）。"""
        if isinstance(s, str):
            s = s.encode()
        self.ser.write(s + b"\r")

    def ymodem_send(self, cmd, timeout):
        """在 pty 中运行 sx/sb 并桥接串口<->pty，直至子进程退出。

        cmd 例：['sx','-q','-b','-X', spl_path] 或 ['sb','-q','-b','--ymodem', img_path]。
        桥接期间关闭终端回显（二进制帧），但串口数据仍写入日志与缓冲区。
        """
        self.tee = False
        master, slave = pty.openpty()
        os.set_blocking(master, False)
        try:
            # 预置 pty slave 为 raw，防止 '\n'->'\r\n' 转换破坏二进制帧
            tty.setraw(slave)
            proc = subprocess.Popen(
                cmd, stdin=slave, stdout=slave, close_fds=True
            )
            os.close(slave)
            start = time.time()
            while proc.poll() is None:
                if time.time() - start > timeout:
                    proc.kill()
                    proc.wait()
                    raise RuntimeError(f"{cmd[0]} 传输超时({timeout}s)")
                r, _, _ = select.select([self.ser, master], [], [], 0.1)
                # 串口 -> pty（给 sx/sb 当输入，含 C 邀请/ACK 帧），同时记录+入缓冲
                if self.ser in r:
                    data = self.ser.read(4096)
                    if data:
                        self._ingest(data)
                        try:
                            os.write(master, data)
                        except OSError:
                            pass
                # pty -> 串口（sx/sb 发出的数据帧/文件内容）
                if master in r:
                    try:
                        data = os.read(master, 4096)
                    except OSError:
                        data = b""
                    if data:
                        self.ser.write(data)
            rc = proc.wait()
            if rc != 0:
                raise RuntimeError(f"{cmd[0]} 退出码 {rc}")
            return rc
        finally:
            self.tee = True
            try:
                os.close(master)
            except OSError:
                pass


# ----------------------------------------------------------------------------
# expect：read_until 的封装，超时直接 abort
# ----------------------------------------------------------------------------
def expect(uart, patterns, timeout, label):
    pat, _ = uart.read_until(patterns, timeout, label)
    if pat is None:
        abort(f"等待「{label}」超时({timeout}s)")
    return pat


# ----------------------------------------------------------------------------
# 主流程
# ----------------------------------------------------------------------------
def do_flow(uart, args):
    # ---------------- S1: 等 BootROM banner 并累计 C 邀请 ----------------
    print("======== Wait for Reboot (BootROM) ========", flush=True)
    cpat = ("C{%d,}" % args.c_threshold).encode()
    pat, _ = uart.read_until([rb"\(C\)StarFive", cpat], 120, "bootrom_banner")
    if pat is None:
        abort("未检测到 BootROM '(C)StarFive' 或足够多的 'C' 邀请")
    if pat != cpat:
        # 收到了 banner，继续收集足够多的 C 邀请
        expect(uart, [cpat], 120, "C_invitation")

    # ---------------- S2: sx 上传 SPL ----------------
    print("======== Send SPL via sx ========", flush=True)
    uart.ymodem_send(["sx", "-q", "-b", "-X", args.spl_normal], timeout=180)
    expect(uart, [b"Minimum SPL"], 60, "spl_banner")

    # ---------------- S3: 跳过 Recovery 菜单并选择引导模式 ----------------
    expect(uart, [rb"\(Press any key to skip\)"], 60, "recovery_countdown")
    # 倒计时只有约 5s，立即回车跳过
    uart.send_line("")
    expect(uart, [b"Skipping Recovery Menu"], 30, "skip_recovery")
    expect(uart, [b"select> "], 30, "boot_mode_menu")
    uart.send_line(args.boot_mode)
    if args.boot_mode == "2":
        confirm = rb"\(TBT only\)"
    else:
        confirm = rb"\(SBI \+ TBT\)"
    expect(uart, [confirm], 30, "boot_mode_confirm")

    # ---------------- S4/S5/S6: 按模式分次 ymodem 传文件 ----------------
    if args.boot_mode == "1":
        # 先 SBI，再等 SPL 自动进入下一轮接收，再 TBT
        expect(uart, [b"Transfer SBI image via YMODEM now"], 60, "ymodem_sbi")
        print("======== Send SBI via sb ========", flush=True)
        uart.ymodem_send(["sb", "-q", "-b", "--ymodem", args.sbi_img], timeout=180)
        expect(uart, [b"Done!"], 30, "sbi_done")

        expect(uart, [b"Transfer TBT image via YMODEM now"], 60, "ymodem_tbt")
        print("======== Send TBT via sb ========", flush=True)
        uart.ymodem_send(["sb", "-q", "-b", "--ymodem", args.tbt_img], timeout=180)
        expect(uart, [b"Done!"], 30, "tbt_done")
    else:
        # 模式 2：TBT only
        expect(uart, [b"Transfer TBT image via YMODEM now"], 60, "ymodem_tbt")
        print("======== Send TBT via sb ========", flush=True)
        uart.ymodem_send(["sb", "-q", "-b", "--ymodem", args.tbt_img], timeout=180)
        expect(uart, [b"Done!"], 30, "tbt_done")

    # ---------------- S7: 等待引导完成 ----------------
    expect(uart, [b"Booting SBI"], 60, "boot_done")

    # ---------------- S8: 分流 ----------------
    if args.auto_script:
        run_auto_script(uart, args.auto_script)
    else:
        interactive_console(uart)


# ----------------------------------------------------------------------------
# S8a: 自动化脚本（Python exec + 注入 ctx）
# ----------------------------------------------------------------------------
class AutoCtx:
    """注入给 --auto_script 的交互对象。

    用法示例（my_auto.py）：
        ctx.write_line("uname -a")
        if ctx.expect(r"Linux", 10):
            print("kernel banner ok")
        data = ctx.wait(2)          # 收集 2 秒内输出
    """

    def __init__(self, uart):
        self._uart = uart

    def read_until(self, pattern, timeout, label=None):
        """等待串口出现匹配 pattern 的正则。命中返回 True，超时返回 False。"""
        pat, _ = self._uart.read_until([pattern], timeout, label)
        return pat is not None

    def expect(self, pattern, timeout, label=None):
        """同 read_until，但超时会抛异常终止脚本。"""
        expect(self._uart, [pattern], timeout, label or pattern)

    def write(self, data):
        """发送原始字节到串口。"""
        if isinstance(data, str):
            data = data.encode()
        self._uart.send(data)

    def write_line(self, s):
        """发送一行（自动补回车）。"""
        self._uart.send_line(s)

    def wait(self, sec):
        """阻塞收集串口输出 sec 秒，返回期间累计接收的原始字节。"""
        self._uart.pump(sec)
        return bytes(self._uart.buffer)


def run_auto_script(uart, path):
    if not os.path.isfile(path):
        abort(f"自动化脚本不存在: {path}")
    print(f"\n======== Run auto script: {path} ========", flush=True)
    with open(path, "r", encoding="utf-8") as f:
        src = f.read()
    ns = {"ctx": AutoCtx(uart), "__name__": "__auto__"}
    exec(compile(src, path, "exec"), ns)
    print("\n======== Auto script finished ========", flush=True)


# ----------------------------------------------------------------------------
# S8b: 交互式 raw 转发控制台（Ctrl-] 本地退出，其余按键透传串口）
# ----------------------------------------------------------------------------
def interactive_console(uart):
    fd = sys.stdin.fileno()
    old = termios.tcgetattr(fd)
    print("\n==== Interactive console (Ctrl-] to exit) ====", flush=True)
    try:
        tty.setraw(fd)
        while True:
            r, _, _ = select.select([sys.stdin, uart.ser], [], [], 0.2)
            if uart.ser in r:
                data = uart.ser.read(4096)
                if not data:
                    break
                uart._ingest(data)  # 记日志 + 回显到终端
            if sys.stdin in r:
                data = os.read(fd, 4096)
                if not data:
                    break
                if b"\x1d" in data:  # Ctrl-]
                    print("\n[console] Ctrl-] pressed, exit", flush=True)
                    break
                data = data.replace(b"\x1d", b"")
                if data:
                    uart.ser.write(data)
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)
        print("\n[console] exit", flush=True)


# ----------------------------------------------------------------------------
# 入口
# ----------------------------------------------------------------------------
def main():
    argparser = argparse.ArgumentParser(description="JH7110 full flash test script")
    argparser.add_argument("--boot_mode", choices=["1", "2"], default="1",
                           help="引导模式: 1=SBI+TBT, 2=TBT only")
    argparser.add_argument("--spl_normal", default="./firmware/spl.bin.normal.out",
                           help="SPL 文件路径")
    argparser.add_argument("--sbi_img", default="./firmware/fw_dynamic.bin.img",
                           help="SBI 镜像文件路径")
    argparser.add_argument("--tbt_img", default="./output/kernel.bin.img",
                           help="TBT 镜像文件路径")
    argparser.add_argument("--port", default="/dev/ttyACM0", help="串口设备路径")
    argparser.add_argument("--baud", type=int, default=115200, help="串口波特率")
    argparser.add_argument("--trace", default="/tmp/full_trace.bin",
                           help="完整日志文件路径")
    argparser.add_argument("--c_threshold", type=int, default=10,
                           help="累计收集 'C' 邀请的阈值")
    argparser.add_argument("--auto_script", default=None,
                           help="引导完成后要执行的自动化 Python 脚本（可选）")
    args = argparser.parse_args()

    # 文件存在性校验
    if not os.path.isfile(args.spl_normal):
        abort(f"SPL 文件不存在: {args.spl_normal}")
    if args.boot_mode == "1" and not os.path.isfile(args.sbi_img):
        abort(f"SBI 镜像文件不存在: {args.sbi_img}")
    if not os.path.isfile(args.tbt_img):
        abort(f"TBT 镜像文件不存在: {args.tbt_img}")

    print("======== Milk-V Mars UART Serial Test Script ========")
    print(f"[tool] 串口={args.port}@{args.baud}")
    print(f"[tool] SPL bin={args.spl_normal} ({os.path.getsize(args.spl_normal)}B)")
    if args.boot_mode == "1":
        print(f"[tool] SBI img={args.sbi_img} ({os.path.getsize(args.sbi_img)}B)")
    print(f"[tool] TBT img={args.tbt_img} ({os.path.getsize(args.tbt_img)}B)")
    print(f"[tool] 完整日志={args.trace}")
    if args.auto_script:
        print(f"[tool] 引导后执行自动化脚本={args.auto_script}")
    print("", flush=True)

    ser = serial.Serial(args.port, args.baud, timeout=0)
    trace = Trace(args.trace)
    uart = Uart(ser, trace)
    try:
        do_flow(uart, args)
        print("\n[tool] done.", flush=True)
    except SystemExit:
        raise
    except Exception as e:
        print(f"\n[tool] 出错: {e}", flush=True)
        raise SystemExit(1)
    finally:
        ser.close()
        trace.close()


if __name__ == "__main__":
    main()
