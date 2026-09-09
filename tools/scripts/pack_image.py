#!/usr/bin/env python3
# 镜像加头脚本
# ```
# 偏移 0    magic    (4B)   OpenSBI: 0x4942534f ("OSBI") / 
#                           trd-boot: 0x54424452 ("TRDB")
# 偏移 4    version  (4B)   当前恒 1（预留：加 hash/entry 等字段时不改 magic）
# 偏移 8    size     (4B)   载荷字节数（不含头）
# 偏移 12   crc32    (4B)   仅对载荷字节计算（不含头）
# ```

import argparse
import os
import zlib

argparser = argparse.ArgumentParser(description="Make image for Milk-V Mars MinimumSPL");
argparser.add_argument("--sbi", type=str, default="", help="Path to SBI binary");
argparser.add_argument("--tbt", type=str, default="", help="Path to TBT binary");
argparser.add_argument("--output", type=str, default="",help="Path to output image");

args = argparser.parse_args()

def parse_output_path(input_path, output_dir):
    # 默认输出到输入文件相同目录
    # 若指定了输出目录，则输出到输出目录
    # 无论哪种输出，文件名都为源文件名(带类型后缀)+".img"
    output_file = os.path.basename(input_path) + ".img"
    if output_dir == "":
        return os.path.join(os.path.dirname(input_path), output_file)
    else:
        os.makedirs(output_dir, exist_ok=True)
        return os.path.join(output_dir, output_file)

def create_image(input_path, magic):
    with open(input_path, "rb") as f:
        data = f.read()
    size = len(data)
    crc32 = zlib.crc32(data) & 0xffffffff
    header = magic + (1).to_bytes(4, "little") + size.to_bytes(4, "little") + crc32.to_bytes(4, "little")
    return header + data, size, crc32

if __name__ == "__main__":
    if args.sbi == "" and args.tbt == "":
        print("Error: Need at least one of --sbi or --tbt")
        exit(1)
    
    if args.sbi != "":
        if (os.path.exists(args.sbi) == False):
            print(f"Error: SBI binary not found: {args.sbi}")
        else:
            output_path = parse_output_path(args.sbi, args.output)

            image, size, crc32 = create_image(args.sbi, b"OSBI")
            
            with open(output_path, "wb") as f:
                f.write(image)

            print(f"Created SBI image: {output_path} (size: {size} bytes, crc32: {crc32:#08x})")
    if args.tbt != "":
        if (os.path.exists(args.tbt) == False):
            print(f"Error: TBT binary not found: {args.tbt}")
        else:
            output_path = parse_output_path(args.tbt, args.output)

            image, size, crc32 = create_image(args.tbt, b"TRDB")
            
            with open(output_path, "wb") as f:
                f.write(image)

            print(f"Created TBT image: {output_path} (size: {size} bytes, crc32: {crc32:#08x})")