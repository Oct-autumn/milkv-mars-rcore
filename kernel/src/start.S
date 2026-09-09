    .section .text.entry
    .globl _start
 _start:
    /* 接收来自 SBI 的参数 */
    /* a0: hartid, a1: dtb, a2: boot stack top */

    /* Set up the stack pointer */
    la sp, _boot_stack_top

    /* Call the main function */
    call main

    /* If main returns, exit the program */
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