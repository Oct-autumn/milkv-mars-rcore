    .section .data
    .align 12                 # 4 KiB 对齐，不能落 .bss
    .globl early_page_table
early_page_table:
    .zero 8                     # root[0]
    .quad 0x100000CF            # root[1]   = 恒等映射 PA [0x40000000, 0x80000000)
    .zero (257 - 2) * 8         # root[2..257) 全部无效
    .quad 0x100000CF            # root[257] = 高映射同一段 PA
    .zero (512 - 258) * 8       # root[258..512) 全部无效

    .section .text.entry
    .globl _start
 _start:
    /* 接收来自 SBI 的参数 */
    /* a0: hartid, a1: dtb, a2: boot stack top */

    /* 1. 取页表物理地址 */
    .option push
    .option norelax             # 禁止gp松弛，否则 la 指令可能会被优化成 lui + addi，导致地址不正确
    lla t0, early_page_table
    .option pop

    /* 2. 设置 satp 寄存器 */
    srli t0, t0, 12              # 取 PPN
    li   t1, 8
    slli t1, t1, 60              # mode = 8 (Sv39) << 60
    or   t0, t0, t1
    csrw satp, t0
    sfence.vma

    /* 3. 取高半区入口的“物理地址”，加上 K_V_MEM_OFFSET 得到其高 VA。
     *    注意：必须对 high_entry 这条标签取地址，不能对 auipc 自身取 PC——
     *    否则 jr 会跳回 auipc 的高位别名，导致偏移被叠加两次。 */
    .option push
    .option norelax             # 同理，禁止 gp 松弛，保证取到真实物理地址
    lla t0, high_entry
    .option pop
    li t1, K_V_MEM_OFFSET           # 内核虚拟地址偏移量
    add t0, t0, t1                  # 入口物理地址 + 偏移量 = 高入口虚拟地址
    jr    t0                        # 跳到高半区入口，PC 从此为高 VA

high_entry:
    /* 4. 此时 PC 已是高 VA，la 解析为高 VA */
    la sp, _boot_stack_top

    /* 5. 调用 main 函数 */
    call main

    /* X. 退出程序 */
    li a7, 93          /* syscall: exit */
    li a0, 0          /* exit code 0 */
    ecall

    /* BSS Stack Section */
    .section .bss.stack
    .globl _boot_stack_lower_bound
_boot_stack_lower_bound:
    /* 16KB stack space */
    .space 0x4000
    .globl _boot_stack_top
_boot_stack_top: