GP_OFFSET = 0*8
SSTATUS_OFFSET = 32*8
SEPC_OFFSET = 33*8
FP_OFFSET = (32 + 2)*8
FCSR_OFFSET = (32 + 2 + 32)*8
K_SP_OFFSET = (32 + 2 + 32 + 1)*8
TRAP_HANDLER_OFFSET = (32 + 2 + 32 + 2)*8

.altmacro
/* 定义一个宏：用于保存通用寄存器 */
.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm
/* 定义一个宏：用于保存浮点寄存器 */
.macro SAVE_FP n
    fsd f\n, (FP_OFFSET + \n*8)(sp)
.endm

/* 定义一个宏：用于加载通用寄存器 */
.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm
/* 定义一个宏：用于加载浮点寄存器 */
.macro LOAD_FP n
    fld f\n, (FP_OFFSET + \n*8)(sp)
.endm

    .section .text.u_trampoline
    .globl __u_alltraps
    .globl __u_restore
    .align 2

/*
 * U态 Trap 进入 S态
 * 根据我们在 init() 中设置的中断向量（stvec），当发生中断或异常时，CPU 会自动跳转到此标签处
 */
__u_alltraps:
    # 此处只有U态Trap会进入。应用程序第一次运行时 在__u_restore中将sscratch置为其地址空间中用来存
    # 放中断上下文的一处常数地址（见 app_init_context() 和 __u_restore）
    csrrw sp, sscratch, sp
    # 交换sp与sscratch后，sp->*TrapContext，sscratch->用户栈顶

    SAVE_GP 1  # 保存 ra
    # 跳过 sp(x2)，此时它的内容指向了内核栈，我们稍后保存它原来的内容（在sscratch中）
    SAVE_GP 3  # 保存 gp
    # TODO(gp/tp): 当前不交换 gp/tp。已确认两个链接脚本均无 __global_pointer$，
    #   且内核/用户程序都不使用 TLS，故不存在 gp/tp 相对寻址；但一旦启用链接松弛
    #   (-mrelax)、TLS 或 stack-protector，就必须在此保存/恢复 gp、tp 并在内核/
    #   用户态之间切换。__restore 中对应位置同理，待后续讨论后统一处理。
    # 跳过 tp(x4)，应用程序用不到它
    # 保存 x5~x31
    # 此处使用伪指令构造一个循环，保存 x5~x31
    .set n, 5
    .rept 27
        SAVE_GP %n
        .set n, n+1
    .endr
    # 保存 f0~f31
    .set n, 0
    .rept 32
        SAVE_FP %n
        .set n, n+1
    .endr

    # 此时我们可以随意使用 t0~t6 了，因为它们被保存在内核栈上的 TrapContext 中
    # 保存 sstatus 和 sepc （中断前的特权级状态和程序计数器）
    csrr t0, sstatus
    csrr t1, sepc
    sd t0, SSTATUS_OFFSET(sp)  # 保存 sstatus
    sd t1, SEPC_OFFSET(sp)  # 保存 sepc

    # 保存被中断的 sp（来自 U 态时为用户 sp，来自 S 态时为内核 sp）。
    # 两条路径都被统一成"sscratch 持有被中断 sp"，故此处无需分支。
    csrr t2, sscratch
    sd t2, 2*8(sp)

    # 保存 fcsr
    csrr t3, fcsr
    sd t3, FCSR_OFFSET(sp)

    # 加载trap_handler地址到 t0 暂存
    ld t0, TRAP_HANDLER_OFFSET(sp)

    # 转到内核栈（SAFE: 先读后写）
    ld sp, K_SP_OFFSET(sp)

    # 跳转到内核 trap_handler
    jr t0

/*
 * U态 Trap 退出 S态
 *
 * 由 trap_return() 调用：
 * - a0: TrapContext 的地址
 */
__u_restore:
    # 恢复 sscratch，指向 TrapContext
    csrw sscratch, a0
    # sp 指向 TrapContext，开始恢复
    mv sp, a0

    # 取 sstatus、sepc，并用 t3 记录本次 trap 的来源特权级（SPP）：
    #   0 表示来自 U 态，非 0 表示来自 S 态。
    # 这里必须用 TrapContext 中的 sstatus 副本，而不能读硬件 sstatus：嵌套 trap
    # 返回时硬件 SPP 会被内层 sret 改写，只有这里的副本始终是本次 trap 的来源。
    ld t0, SSTATUS_OFFSET(sp)  # 取 sstatus
    ld t1, SEPC_OFFSET(sp)  # 取 sepc
    csrw sstatus, t0
    csrw sepc, t1

    # 恢复浮点寄存器
    .set n, 0
    .rept 32
        LOAD_FP %n
        .set n, n+1
    .endr
    # 恢复 fcsr
    ld t3, FCSR_OFFSET(sp)
    csrw fcsr, t3

    LOAD_GP 1  # 恢复 ra
    # 跳过 sp(x2)，稍后从 TrapContext 中恢复
    LOAD_GP 3  # 恢复 gp
    # 跳过 tp(x4)，应用程序用不到它
    # 恢复 x5~x31
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr

    # 直接从 TrapContext 恢复 sp：U 态路径得到用户 sp，S 态路径得到被中断的
    # 内核 sp，两条路径一致，因此无需再借助 sscratch 交换。
    # （ld 的地址计算在写 rd 之前完成，故 ld sp, 2*8(sp) 合法。）
    ld sp, 2*8(sp)
    sret  # 返回（目标特权级由恢复的 sstatus.SPP 决定）

    .section .text.k_trampoline
    .globl __k_alltraps
    .globl __k_restore
    .align 2

__k_alltraps:
__k_restore:
    # TODO: 尚未实现的 S态 中断处理
    wfi
    j __k_alltraps
