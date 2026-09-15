.altmacro
/* 定义一个宏：用于保存 s 寄存器 */
.macro SAVE_SN n
    sd s\n, (\n+2)*8(a0)
.endm
/* 定义一个宏：用于加载 s 寄存器 */
.macro LOAD_SN n
    ld s\n, (\n+2)*8(a1)
.endm

    .section .text
    .globl __switch
__switch:
    # a0: 保存上下文的地址
    # a1: 加载上下文的地址

    # __switch 作为一个func被上层rust代码调用。为了切换任务上下文，它要做两件事：
    # [Stage 1]
    # 由于 rust 的函数调用约定，a0、a1、ra、sp 都是 caller-saved 寄存器，
    # 因此在 rust 代码中调用 __switch 时，这些寄存器的值已经被保存到内核栈上了。
    # [Stage 2]
    # 保存 栈指针(sp)、返回地址(ra)和rust调用约定中，被调用者需要保存的寄存器(s0-s11)
    # 到 a0 指向的内存中（即当前任务的 TaskContext 中）。
    sd sp, 1*8(a0)
    sd ra, 0*8(a0)
    .set n, 0
    .rept 12
        SAVE_SN %n
        .set n, n+1
    .endr
    # [Stage 3]
    # 从 a1 指向的内存中（即下一个任务的 TaskContext 中）加载 sp、ra 和 s0-s11 到寄存器中。
    .set n, 0
    .rept 12
        LOAD_SN %n
        .set n, n+1
    .endr
    ld ra, 0*8(a1)
    ld sp, 1*8(a1)
    # [Stage 4]
    # 返回到下一个任务的上下文中，继续执行。
    ret