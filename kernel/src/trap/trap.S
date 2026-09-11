.altmacro
/* 定义一个宏：用于保存通用寄存器 */
.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm

/* 定义一个宏：用于加载通用寄存器 */
.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm

# TrapContext 结构体的大小为 32个通用寄存器+ sstatus + sepc = 34 个寄存器
TRAP_CONTEXT_SIZE = 34*8  # 每个寄存器占 8 字节

    .section .text
    .globl __alltraps
    .globl __restore
    .align 2

/*
 * Trap 进入内核态
 * 根据我们在 init() 中设置的中断向量（stvec），当发生中断或异常时，CPU 会自动跳转到此标签处
 */
__alltraps:
    # TODO(S-mode trap): 这里假设所有 trap 都来自 U 态——sscratch 中预先存放的是
    #   内核栈顶。若在 S 态（例如 syscall 处理期间）发生异常/中断，csrrw 会把 sp
    #   切成用户栈并在其上保存上下文，导致栈错误。当前所有 S 态异常都会被
    #   trap_handler 的兜底分支杀掉应用，因此尚未暴露；引入时钟中断/嵌套中断前需处理。
    csrrw sp, sscratch, sp  # 交换 sp 和 sscratch 的值
    # 此时 sp 指向内核栈的顶部，sscratch 指向用户栈的顶部

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

    # 此时我们可以随意使用 t0~t6 了，因为它们被保存在内核栈上的 TrapContext 中
    # 保存 sstatus 和 sepc （中断前的特权级状态和程序计数器）
    csrr t0, sstatus
    csrr t1, sepc
    sd t0, 32*8(sp)  # 保存 sstatus
    sd t1, 33*8(sp)  # 保存 sepc

    # 保存 sp 的原始值（用户栈的顶部），它在 sscratch 中
    csrr t2, sscratch
    sd t2, 2*8(sp)  # 保存原始 sp

    # 现在我们已经保存了所有需要的寄存器，接下来调用 Rust 的 trap_handler 函数
    mv a0, sp  # 将 TrapContext 的地址传递给 trap_handler
    call trap_handler

/*
 * Trap 退出内核态
 */
__restore:
    # trap_handler 返回后，我们需要恢复寄存器的值
    mv sp, a0  # trap_handler 返回时，a0 中保存了 TrapContext 的地址

    # 恢复 sstatus、sepc、sscratch
    ld t0, 32*8(sp)  # 取 sstatus
    ld t1, 33*8(sp)  # 取 sepc
    ld t2, 2*8(sp)  # 取原始 sp
    csrw sstatus, t0
    csrw sepc, t1
    csrw sscratch, t2

    LOAD_GP 1  # 恢复 ra
    # 跳过 sp(x2)，此时它的内容指向了内核栈，我们稍后恢复它原来的内容（在sscratch中）
    LOAD_GP 3  # 恢复 gp
    # 跳过 tp(x4)，应用程序用不到它
    # 恢复 x5~x31
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr

    # 回收 TrapContext 的空间
    addi sp, sp, TRAP_CONTEXT_SIZE
    # 恢复 sscratch 和 sp 的值
    csrrw sp, sscratch, sp  # 交换 sp 和 sscratch 的值
    sret  # 返回用户态
