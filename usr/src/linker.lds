OUTPUT_ARCH(riscv)
ENTRY(_start)

BASE_ADDRESS = 0x40000000;

SECTIONS
{
    . = BASE_ADDRESS;
    .text : {
        *(.text.entry)
        *(.text .text.*)
    }
    . = ALIGN(4K);
    .rodata : {
        *(.rodata .rodata.*)
    }
    . = ALIGN(4K);
    .data : {
        *(.data .data.*)
    }
    .bss : {
        start_bss = .;
        *(.bss .bss.*)
        end_bss = .;
    }
    /DISCARD/ : {
        *(.eh_frame)
        *(.debug*)
    }
}