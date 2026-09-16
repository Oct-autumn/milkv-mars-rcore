.altmacro
/* 定义一个宏：用于保存通用寄存器 */
.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm
/* 定义一个宏：用于保存浮点寄存器 */
.macro SAVE_FP n
    fsd f\n, (32 + 2 + \n)*8(sp)
.endm

/* 定义一个宏：用于加载通用寄存器 */
.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm
/* 定义一个宏：用于加载浮点寄存器 */
.macro LOAD_FP n
    fld f\n, (32 + 2 + \n)*8(sp)
.endm

# TrapContext 结构体的大小为 32个通用寄存器 + sstatus + sepc + 32个浮点寄存器 + fcsr = 67 个寄存器
TRAP_CONTEXT_SIZE = 67*8  # 每个寄存器占 8 字节

# sstatus.SPP（Supervisor Previous Privilege）位：trap 前的特权级，bit 8 为 1 表示来自 S 态
.set SSTATUS_SPP, 0x100

    .section .text
    .globl __alltraps
    .globl __restore
    .align 2

/*
 * Trap 进入内核态
 * 根据我们在 init() 中设置的中断向量（stvec），当发生中断或异常时，CPU 会自动跳转到此标签处
 */
__alltraps:
    # 来源判别：约定 sscratch 在 U 态运行期间保存"当前任务的内核栈顶"，在内核
    #   （S 态）运行期间固定为 0（由本轮保存完上下文后的 `csrw sscratch, zero`
    #   建立并维持）。于是交换后 sp 非 0 即来自 U 态、为 0 即来自 S 态。
    #   之所以用 sp 的零/非零而不是 sstatus.SPP 判别：trap 刚进入时 32 个通用
    #   寄存器全部尚未保存，没有 scratch 寄存器可供 csrr 读出 SPP；而 sp 自身
    #   可以直接用 bnez 判断，整个过程不破坏任何寄存器。
    csrrw sp, sscratch, sp
    bnez sp, .Ltrap_save
    # 来自 S 态：把 sp 换回来，沿用被中断时的内核栈（不交换）。若在此处交换，
    #   TrapContext 会被压到用户栈上，或覆盖外层 trap 的 TrapContext。
    csrrw sp, sscratch, sp  # sp = 被中断的内核 sp，sscratch = 0
    csrw sscratch, sp       # 与 U 态路径统一为“sscratch = 被中断的 sp”

.Ltrap_save:
    # 分配一个 TrapContext 结构体的空间
    addi sp, sp, -TRAP_CONTEXT_SIZE

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
    sd t0, 32*8(sp)  # 保存 sstatus
    sd t1, 33*8(sp)  # 保存 sepc

    # 保存被中断的 sp（来自 U 态时为用户 sp，来自 S 态时为内核 sp）。
    # 两条路径都被统一成"sscratch 持有被中断 sp"，故此处无需分支。
    csrr t2, sscratch
    sd t2, 2*8(sp)

    # 保存 fcsr
    csrr t3, fcsr
    sd t3, (32 + 2 + 32)*8(sp)

    # 建立"内核期 sscratch == 0"不变量：此后直到 __restore 返回用户态，sscratch
    # 一直为 0，供嵌套 trap 的 __alltraps 判别来源。
    csrw sscratch, zero

    # 现在我们已经保存了所有需要的寄存器，接下来调用 Rust 的 trap_handler 函数
    mv a0, sp  # 将 TrapContext 的地址传递给 trap_handler
    call trap_handler

/*
 * Trap 退出内核态
 */
__restore:
    # trap_handler 返回后，我们需要恢复寄存器的值
    
    # 在多道程序OS中，不再需要这条指令
    # 因为 __restore 不再是以函数形式调用，它的执行只有两种场景：
    # 1. trap_handler 返回时：此时sp仍然指向TrapContext的地址不变，该指令无用；
    # 2. 执行首个应用程序时：从TaskContext恢复寄存器时，sp已经被正确的指向内核栈地址
    #   （其中有init_app_cx创建的初始TrapContext），该指令无用。
    #mv sp, a0  # trap_handler 返回时，a0 中保存了 TrapContext 的地址

    # 取 sstatus、sepc，并用 t3 记录本次 trap 的来源特权级（SPP）：
    #   0 表示来自 U 态，非 0 表示来自 S 态。
    # 这里必须用 TrapContext 中的 sstatus 副本，而不能读硬件 sstatus：嵌套 trap
    # 返回时硬件 SPP 会被内层 sret 改写，只有这里的副本始终是本次 trap 的来源。
    ld t0, 32*8(sp)  # 取 sstatus
    ld t1, 33*8(sp)  # 取 sepc
    andi t3, t0, SSTATUS_SPP
    csrw sstatus, t0
    csrw sepc, t1

    # 按来源设定 sscratch，恢复相应不变量：
    #   返回 U 态 -> sscratch = 内核栈顶（= 当前 sp + TRAP_CONTEXT_SIZE）
    #   返回 S 态 -> sscratch 继续置 0
    bnez t3, .Lrestore_to_s
    addi t0, sp, TRAP_CONTEXT_SIZE
    csrw sscratch, t0
    j .Lrestore_common

.Lrestore_to_s:
    # 当前尚不可达：S 态 trap 一律被 trap_handler 判为内核 bug 并 panic，不会返回。
    # 保留此分支是为了维持 __restore 的语义完整——将来若实现内核抢占，返回 S 态
    # 时必须继续维持"内核期 sscratch == 0"的不变量。
    csrw sscratch, zero

.Lrestore_common:
    # 恢复浮点寄存器
    .set n, 0
    .rept 32
        LOAD_FP %n
        .set n, n+1
    .endr
    # 恢复 fcsr
    ld t3, (32 + 2 + 32)*8(sp)
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
