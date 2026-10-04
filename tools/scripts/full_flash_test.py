#!/usr/bin/env python3
"""
JH7110 上板测试脚本（Milk-V Mars）

====================================================================
运行步骤：
- 等待上电后 BootROM 稳定，累计收集足够多的 'C' 邀请
- 用内置 XMODEM-CRC 发送端上传 SPL .normal.out
- 等待 SPL 启动，跳过 Recovery 菜单，选择引导模式（1:SBI+TBT/2:TBT only）
- 等待 SPL 进入 ymodem 接收阶段
- 用内置 YMODEM 发送端上传镜像（按引导模式发送不同文件）
- 等待引导
- 接收后续串口输出，读取键盘输入反馈到串口
- 全程记录日志到 full_trace.bin
====================================================================

串口链路说明（基于 SPL 实测输出）：
- BootROM 上电后持续输出 '(C)StarFive' 与 'C' 邀请（XMODEM-CRC 握手），
  上传 SPL 用 XMODEM。
- SPL 启动后打印 '==== Minimum SPL - v0.1 ===='，随后进入 Recovery
  倒计时（'Entering Recovery Menu in 05 seconds ... (Press any key to skip)'），
  任意键（回车）可跳过。
- 跳过 Recovery 后出现 '==== Select Boot Mode ====' 菜单：
    0: Boot From Flash
    1: Boot From UART (SBI + TBT)
    2: Boot From UART (TBT only)
  输入 1/2 并回车确认。
- 模式 1（SBI+TBT）：先 'Transfer SBI image via YMODEM now...'（YMODEM 传 SBI），
  收到 'Done!' 后 SPL 自动输出 'Transfer TBT image via YMODEM now...'（YMODEM 传 TBT）。
- 模式 2（TBT only）：'Reading SBI image from flash ...Done!' 后
  直接 'Transfer TBT image via YMODEM now...'（YMODEM 传 TBT）。
- 传输成功标志：'Recv success, total size: ...Bytes.' + 'Done!'。
- 最后 'Booting SBI ...' 表示 SPL 已把控制权交给 SBI/三级引导。

协议实现说明（本脚本自带发送端，**不再依赖 lrzsz 的 sx/sb**）：
- 接收端源码见 mars-trd-boot-dev/spl/src/utils/y_modem.c，发送端握手与之对齐：
    * 等接收端邀请：'C'(0x43)=CRC 模式，NAK(0x15)=校验和模式；
    * YMODEM 块0 必须携带 "文件名\\0十进制大小 mtime mode serial\\0"，
      接收端据此裁剪末块填充（否则会把填充字节写进 flash）；
    * 接收端 ACK 块0 后会补发一次 'C'，发送端等它再发数据块；
    * 数据块 CRC-16/XMODEM（poly 0x1021，初值 0，高字节在前），NAK/超时重传；
    * 第一个 EOT 被 ACK 后接收端再发 'C'，随后以空块0（或第二个 EOT）收尾。
- 数据块默认 1024B（接收端支持 STX 包，与 sb -k 一致），可用 --ymodem_block 改回 128B；
  XMODEM 默认 128B（与 sx 一致，BootROM 实测可用）。
- 传输阶段串口数据仍全部写入 full_trace.bin（字节级连续），并在终端显示
  进度条（百分比 / 字节数 / 速率 / 预计剩余时间 / 重传次数）。
"""


import argparse
import contextlib
import os
import re
import select
import shutil
import sys
import termios
import time
import tty

import serial

import serial


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
        with contextlib.suppress(Exception):
            self.f.close()


# ----------------------------------------------------------------------------
# Uart 会话：封装串口读取、日志、实时回显与跨阶段缓冲区
#
# 核心约定：所有从串口读到的数据都走 _ingest() ->
#   1) 写入日志 full_trace.bin（全程连续）
#   2) 追加进 self.buffer（供 read_until 跨阶段匹配，避免丢数据）
#   3) 若 self.tee 为真，实时回显到终端（协议传输阶段关闭，避免二进制乱码）
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
            with contextlib.suppress(OSError):
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()
        return len(data)

    def pump(self, timeout):
        """阻塞读取串口至多 timeout 秒，返回期间累计接收的字节数。"""
        end = time.time() + timeout
        total = 0
        while time.time() < end:
            r, _, _ = select.select([self.ser], [], [], 0.05)
            if r:
                if data := self.ser.read(4096):
                    total += self._ingest(data)
                else:
                    # 串口可读但读到空 => 连接异常/断开
                    raise serial.SerialException("串口读取到空数据，可能已断开")
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
                if m := re.search(p, buf):
                    consumed_end = m.end()
                    consumed = bytes(self.buffer[:consumed_end])
                    del self.buffer[:consumed_end]
                    if label:
                        print(f"\n[{label}] 命中: {p!r}", flush=True)
                    return p, consumed
            if _ := self.pump(0.2):
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


# ----------------------------------------------------------------------------
# 协议常量（XMODEM / YMODEM）
# ----------------------------------------------------------------------------
SOH = 0x01   # 128 字节数据包
STX = 0x02   # 1024 字节数据包
EOT = 0x04   # 传输结束
ACK = 0x06   # 确认
NAK = 0x15   # 否定确认（请求重传）
CAN = 0x18   # 取消传输
SUB = 0x1A   # ^Z，末块填充（CPMEOF）
CHR_C = 0x43  # 'C'，CRC 模式邀请

ACK_B = bytes([ACK])
CAN_B = bytes([CAN])
EOT_B = bytes([EOT])
CHR_C_B = bytes([CHR_C])

# CRC-16/XMODEM：poly 0x1021、初值 0、无输出异或、MSB-first（与接收端 calc_crc16(
# ..., CRC16_CCITT) 一致）。查表实现，120KB 文件开销可忽略。
_CRC16_TABLE = []
for _i in range(256):
    _c = _i << 8
    for _ in range(8):
        _c = ((_c << 1) ^ 0x1021) & 0xFFFF if (_c & 0x8000) else ((_c << 1) & 0xFFFF)
    _CRC16_TABLE.append(_c)


def crc16_xmodem(data):
    """CRC-16/XMODEM（poly 0x1021，初值 0）。"""
    crc = 0
    for b in data:
        crc = ((crc << 8) & 0xFFFF) ^ _CRC16_TABLE[((crc >> 8) ^ b) & 0xFF]
    return crc


def build_packet(header, seq, payload, crc_mode=True):
    """组一个 XMODEM/YMODEM 数据帧：帧头 + 序号 + 序号反码 + 数据 + CRC/校验和。"""
    frame = bytearray([header, seq & 0xFF, (~seq) & 0xFF])
    frame += payload
    if crc_mode:
        c = crc16_xmodem(payload)
        frame += bytes([(c >> 8) & 0xFF, c & 0xFF])
    else:
        frame.append(sum(payload) & 0xFF)
    return bytes(frame)


class TransferError(RuntimeError):
    """协议层传输失败（握手超时、重传耗尽、接收方取消等）。"""


# ----------------------------------------------------------------------------
# Link：协议收发适配层（串口 <-> 字节流）
#
# - 读到的每个字节都走 Uart._ingest()，因此 full_trace.bin 依旧全程连续；
# - 协议期间关闭终端回显（二进制帧会刷屏），close() 时恢复；
# - 优先消费 uart.buffer 中的残留字节（read_until 命中后可能还剩数据），
#   被协议消费的字节会从 buffer 移除，避免污染后续 expect()。
# ----------------------------------------------------------------------------
class Link:
    def __init__(self, uart):
        self.uart = uart
        self._tee = uart.tee
        uart.tee = False

    def close(self):
        self.uart.tee = self._tee

    def _fill(self, timeout):
        """保证 buffer 中至少有 1 字节；超时返回 False。"""
        if self.uart.buffer:
            return True
        end = time.time() + timeout
        while True:
            remain = end - time.time()
            if remain <= 0:
                return False
            r, _, _ = select.select([self.uart.ser], [], [], min(remain, 0.05))
            if not r:
                continue
            data = self.uart.ser.read(4096)
            if not data:
                raise serial.SerialException("串口读取到空数据，可能已断开")
            self.uart._ingest(data)
            if self.uart.buffer:
                return True

    def recv_byte(self, timeout):
        """读取 1 字节；超时返回 b''。"""
        if not self._fill(timeout):
            return b""
        b = bytes(self.uart.buffer[:1])
        del self.uart.buffer[:1]
        return b

    def write(self, data):
        self.uart.ser.write(data)


# ----------------------------------------------------------------------------
# Progress：传输进度显示
#
# - stdout 是终端：'\r' 原地刷新单行进度条（限速 20 次/秒）
# - stdout 被重定向（非 TTY）：按 10% 阶梯各打印一行，避免刷屏
# ----------------------------------------------------------------------------
class Progress:
    def __init__(self, proto, name, total, verbose=False):
        self.proto = proto
        self.raw_name = name
        self.name = name if len(name) <= 24 else f"{name[:11]}...{name[-10:]}"
        self.total = max(1, total)
        self.size = total
        self.verbose = verbose
        self.tty = sys.stdout.isatty()
        self.start = time.time()
        self.last = 0.0
        self.next_pct = 0
        self.last_len = 0
        cols = shutil.get_terminal_size((100, 24)).columns
        self.bar_width = max(16, min(36, cols - 62))

    def _line(self, sent, retries):
        elapsed = max(1e-3, time.time() - self.start)
        pct = min(100, sent * 100 // self.total)
        filled = int(self.bar_width * pct / 100)
        bar = "#" * filled + "-" * (self.bar_width - filled)
        speed = sent / elapsed
        eta = (self.total - sent) / speed if speed > 0 else 0.0
        return (f"[{self.proto}] {self.name} [{bar}] {pct:3d}% "
                f"{sent}/{self.total}B {speed / 1024:6.1f}KB/s ETA{eta:5.1f}s 重传{retries}")

    def _emit(self, text):
        if self.tty:
            pad = " " * max(0, self.last_len - len(text))
            sys.stdout.write("\r" + text + pad)
        else:
            sys.stdout.write(text + "\n")
        sys.stdout.flush()
        self.last_len = len(text)

    def update(self, sent, retries=0):
        now = time.time()
        if self.tty:
            if now - self.last < 0.05 and sent < self.total:
                return
        else:
            pct = sent * 100 // self.total
            if pct < self.next_pct and sent < self.total:
                return
            self.next_pct = (pct // 10 + 1) * 10
        self.last = now
        self._emit(self._line(sent, retries))

    def finish(self, ok, retries=0, blocks=0, note=""):
        elapsed = max(1e-3, time.time() - self.start)
        avg = self.size / elapsed / 1024
        if self.tty:
            sys.stdout.write("\r" + " " * self.last_len + "\r")
            sys.stdout.flush()
        state = "完成" if ok else "失败"
        msg = (f"[{self.proto}] {state}: {self.raw_name} {self.size}B / {blocks} 块, "
               f"用时 {elapsed:.1f}s, 平均 {avg:.1f}KB/s, 重传 {retries} 次")
        if note:
            msg += f"（{note}）"
        print(msg, flush=True)


# ----------------------------------------------------------------------------
# 发送端：XMODEM / YMODEM（自实现，行为对齐 mars-trd-boot-dev 的 SPL 接收端）
# ----------------------------------------------------------------------------
class BaseSender:
    def __init__(self, link, *, block_size, max_retries=10, resp_timeout=2.0,
                 crc_mode=True, verbose=False, progress=None):
        self.link = link
        self.block_size = block_size
        self.max_retries = max_retries
        self.resp_timeout = resp_timeout
        self.crc_mode = crc_mode
        self.verbose = verbose
        self.progress = progress
        self.retries = 0     # 累计重传次数
        self.blocks = 0      # 累计发送的数据块数
        self.last_noise = b""  # 最近一次等待控制字节时吞掉的非控制字节

    # ---- 控制字节收发 ----
    def _wait_ctl(self, timeout, ignore_c=False):
        """等待一个控制字节（ACK/NAK/CAN/EOT/'C'），超时返回 b''。

        ignore_c=True 时把 'C' 也当噪声丢掉：接收端在收到首包之前会每 500ms
        发一次 'C' 邀请，在途的邀请可能晚于数据包到达，若把它当成应答就会
        白白触发一次重传（接收端其实是先发 ACK 再补 'C'，这里只需吃掉邀请）。
        其余字节一律按噪声处理（接收端可能夹带调试文本），最多缓存 4KB，
        verbose 下回显，便于上板排查。
        """
        end = time.time() + timeout
        noise = bytearray()
        while True:
            remain = end - time.time()
            if remain <= 0:
                self._report_noise(noise)
                return b""
            b = self.link.recv_byte(remain)
            if not b:
                self._report_noise(noise)
                return b""
            if b[0] == CHR_C and ignore_c:
                continue
            if b[0] in (ACK, NAK, CAN, EOT, CHR_C):
                self._report_noise(noise)
                return b
            noise += b
            if len(noise) >= 4096:
                self._report_noise(noise)
                return b""

    def _report_noise(self, noise):
        if noise:
            self.last_noise = bytes(noise)
            if self.verbose:
                shown = bytes(noise[:160])
                print(f"\n[proto] 收到非控制字节 {len(noise)}B: {shown!r}", flush=True)
        else:
            self.last_noise = b""

    def _wait_invite(self, timeout):
        """等待接收端握手邀请。'C' => CRC 模式；NAK => 校验和模式。"""
        end = time.time() + timeout
        while True:
            remain = end - time.time()
            if remain <= 0:
                raise TransferError(f"等待接收端 'C' 邀请超时({timeout:.0f}s)")
            b = self.link.recv_byte(remain)
            if not b:
                raise TransferError(f"等待接收端 'C' 邀请超时({timeout:.0f}s)")
            if b[0] == CHR_C:
                self.crc_mode = True
                if self.verbose:
                    print("\n[proto] 收到 'C' 邀请：CRC 模式", flush=True)
                return
            if b[0] == NAK:
                self.crc_mode = False
                if self.verbose:
                    print("\n[proto] 收到 NAK 邀请：校验和模式", flush=True)
                return

    def _send_frame(self, frame, what):
        """发送一帧并等 ACK；NAK / 超时 / 其他控制字节都触发重传。"""
        for attempt in range(1, self.max_retries + 1):
            self.link.write(frame)
            r = self._wait_ctl(self.resp_timeout, ignore_c=True)
            if r == ACK_B:
                return
            if r == CAN_B:
                raise TransferError(f"{what}: 接收端发送 CAN，传输被取消")
            self.retries += 1
            if self.verbose:
                reason = f"0x{r[0]:02X}" if r else "超时"
                extra = f" 最近非控制字节={self.last_noise[:40]!r}" if self.last_noise else ""
                print(f"\n[proto] {what} 未确认({reason})，重传 {attempt}/{self.max_retries}{extra}",
                      flush=True)
        raise TransferError(f"{what}: 重传 {self.max_retries} 次仍未收到 ACK")

    # ---- 数据块 ----
    def _send_data(self, data):
        total = len(data)
        seq = 1
        header = STX if self.block_size == 1024 else SOH
        pad = bytes([SUB]) * self.block_size
        for off in range(0, total, self.block_size):
            chunk = data[off:off + self.block_size]
            if len(chunk) < self.block_size:
                chunk = chunk + pad[: self.block_size - len(chunk)]
            self._send_frame(build_packet(header, seq, chunk, self.crc_mode),
                             f"数据块 {seq & 0xFF}")
            self.blocks += 1
            seq += 1
            if self.progress:
                self.progress.update(min(total, off + self.block_size), self.retries)

    # ---- 结束握手 ----
    def _send_eot(self):
        """发 EOT 并等 ACK；部分接收端会先 NAK 一次要求重发 EOT。"""
        for attempt in range(1, self.max_retries + 1):
            self.link.write(EOT_B)
            r = self._wait_ctl(self.resp_timeout)
            if r == ACK_B:
                return
            if r == CAN_B:
                raise TransferError("EOT: 接收端发送 CAN，传输被取消")
            if self.verbose:
                reason = f"0x{r[0]:02X}" if r else "超时"
                print(f"\n[proto] EOT 未确认({reason})，重发 {attempt}/{self.max_retries}", flush=True)
        raise TransferError(f"EOT: 重发 {self.max_retries} 次仍未收到 ACK")


class XModemSender(BaseSender):
    """XMODEM(-CRC) 发送端：无文件头，数据块序号从 1 开始。"""

    def send(self, data, name="", invite_timeout=60.0):
        self._wait_invite(invite_timeout)
        self._send_data(data)
        self._send_eot()
        return self


class YModemSender(BaseSender):
    """YMODEM(batch) 发送端：块0 文件头 + 数据块 + 双阶段 EOT 收尾。"""

    def send(self, data, name="", invite_timeout=60.0, mtime=None):
        self._wait_invite(invite_timeout)

        # 块0：文件名\0 + "十进制大小 八进制mtime 八进制mode 串号\0"，NUL 补齐 128B。
        # 接收端先跳过 '\0' 再读十进制数字作为文件长度（用于裁剪末块填充）。
        if mtime is None:
            mtime = int(time.time())
        info = f"{name}\0{len(data)} {mtime:o} {0o644:o} 0\0".encode()
        blk0 = info.ljust(128, b"\0")[:128]
        if self.verbose:
            print(f"\n[proto] 块0: {info[:80]!r}", flush=True)
        self._send_frame(build_packet(SOH, 0, blk0, self.crc_mode), "文件头(块0)")

        # 数据阶段邀请：接收端 ACK 块0 后会补发一次 'C'；等不到也照常发数据块
        r = self._wait_ctl(5.0)
        if r != CHR_C_B and self.verbose:
            print(f"\n[proto] 未收到数据阶段 'C'（{r!r}），直接发送数据块", flush=True)

        self._send_data(data)

        # 收尾：第一个 EOT -> ACK 后接收端发 'C' 请求结束包（空块0）
        self._send_eot()
        r = self._wait_ctl(5.0)
        if r == CHR_C_B:
            try:
                self._send_frame(build_packet(SOH, 0, bytes(128), self.crc_mode), "结束包(空块0)")
            except TransferError:
                # 结束包未被确认不影响已传数据（接收端也可能已按 EOT 收尾）
                if self.verbose:
                    print("\n[proto] 结束包未确认，按已完成处理", flush=True)
        elif r != EOT_B:
            # 没等到 'C'：补发一个 EOT 兜底（部分接收端需要两个 EOT）
            self.link.write(EOT_B)
            self._wait_ctl(self.resp_timeout)
        return self


def transfer(uart, proto, path, *, block_size, retries=10, resp_timeout=2.0,
             invite_timeout=60.0, verbose=False):
    """用内置 XMODEM/YMODEM 发送端发送一个文件（含进度显示）。

    成功返回发送端实例（可读 retries/blocks），失败抛 TransferError。
    """
    size = os.path.getsize(path)
    name = os.path.basename(path)
    with open(path, "rb") as f:
        data = f.read()
    if len(data) != size:
        raise TransferError(f"读取 {path} 长度异常: {len(data)} != {size}")

    pgr = Progress(proto.upper(), name, size, verbose=verbose)
    link = Link(uart)
    sender = None
    try:
        cls = XModemSender if proto == "xmodem" else YModemSender
        sender = cls(link, block_size=block_size, max_retries=retries,
                     resp_timeout=resp_timeout, verbose=verbose, progress=pgr)
        sender.send(data, name, invite_timeout=invite_timeout)
        pgr.finish(True, sender.retries, sender.blocks)
        return sender
    except Exception as e:
        # 任何失败都先收尾进度行（串口断开也要避免终端留下半行进度条）
        pgr.finish(False, sender.retries if sender else 0,
                   sender.blocks if sender else 0, note=str(e))
        raise
    finally:
        # 恢复终端回显；不清理 uart.buffer —— 传输结束后 SPL 会立刻打印
        # 'Recv success...'，这些字节必须留给后续 expect()
        link.close()


# ----------------------------------------------------------------------------
# expect：read_until 的封装，超时直接 abort
# ----------------------------------------------------------------------------
def expect(uart, patterns, timeout, label):
    pat, _ = uart.read_until(patterns, timeout, label)
    if pat is None:
        abort(f"等待「{label}」超时({timeout}s)")
    return pat


# ----------------------------------------------------------------------------
# 传输封装：打印阶段标题，失败即 abort
# ----------------------------------------------------------------------------
def do_transfer(uart, args, proto, path, what):
    block_size = args.xmodem_block if proto == "xmodem" else args.ymodem_block
    print(f"\n======== 发送{what}（内置 {proto.upper()}，{block_size}B/包，"
          f"{os.path.getsize(path)}B） ========", flush=True)
    try:
        transfer(uart, proto, path, block_size=block_size, retries=args.retries,
                 resp_timeout=args.resp_timeout, verbose=args.verbose)
    except TransferError as e:
        abort(f"{what} 传输失败: {e}")


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

    # ---------------- S2: XMODEM 上传 SPL ----------------
    do_transfer(uart, args, "xmodem", args.spl_normal, "SPL")
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

    # ---------------- S4/S5/S6: 按模式分次 YMODEM 传文件 ----------------
    if args.boot_mode == "1":
        # 先 SBI，再等 SPL 自动进入下一轮接收，再 TBT
        expect(uart, [b"Transfer SBI image via YMODEM now"], 60, "ymodem_sbi")
        do_transfer(uart, args, "ymodem", args.sbi_img, "SBI")
        expect(uart, [b"Done!"], 30, "sbi_done")

    expect(uart, [b"Transfer TBT image via YMODEM now"], 60, "ymodem_tbt")
    do_transfer(uart, args, "ymodem", args.tbt_img, "TBT")
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
                if data := uart.ser.read(4096):
                    uart._ingest(data)  # 记日志 + 回显到终端
                else:
                    break
            if sys.stdin in r:
                data = os.read(fd, 4096)
                if not data:
                    break
                if b"\x1d" in data:  # Ctrl-]
                    print("\n[console] Ctrl-] pressed, exit", flush=True)
                    break
                if data := data.replace(b"\x1d", b""):
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
    argparser.add_argument("--xmodem_block", type=int, choices=[128, 1024], default=128,
                           help="XMODEM 数据块大小（BootROM 实测 128B 可用，默认 128）")
    argparser.add_argument("--ymodem_block", type=int, choices=[128, 1024], default=1024,
                           help="YMODEM 数据块大小（SPL 支持 1024B，默认 1024）")
    argparser.add_argument("--retries", type=int, default=10,
                           help="单帧最大重传次数（默认 10，对齐 U-Boot）")
    argparser.add_argument("--resp_timeout", type=float, default=2.0,
                           help="等待接收端 ACK/NAK 的超时秒数（默认 2.0）")
    argparser.add_argument("--verbose", action="store_true",
                           help="打印协议细节（邀请模式、重传、块0、非控制字节）")
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
    print(f"[tool] 协议=内置 XMODEM({args.xmodem_block}B)/YMODEM({args.ymodem_block}B)，"
          f"重传上限={args.retries}")
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
