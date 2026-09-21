OUTPUT_ARCH(riscv)
ENTRY(_start)

SECTIONS
{
    . = BASE_ADDRESS;
    skernel = .;

    stext = .;
    .text : {
        /* 入口，必须放在最前面 */
        *(.text.entry)
        . = ALIGN(4K);
        /* U态 trampoline 代码 */
        s_u_trampoline = .;
        *(.text.u_trampoline);
        . = ALIGN(4K);
        /* S态 trampoline 代码 */
        s_k_trampoline = .;
        *(.text.k_trampoline);
        . = ALIGN(4K);
        *(.text .text.*)
    }

    . = ALIGN(4K);
    etext = .;
    srodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }

    . = ALIGN(4K);
    erodata = .;
    sdata = .;
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }

    . = ALIGN(4K);
    edata = .;
    sbss_with_stack = .;
    .bss : {
        *(.bss.stack)
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }

    . = ALIGN(4K);
    ebss = .;
    ekernel = .;
    
    /DISCARD/ : {
        *(.eh_frame)
        *(.debug*)
    }
}
