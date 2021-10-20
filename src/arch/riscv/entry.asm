    .section .text.entry
    .globl _start
_start:
    // Setup page table
    lui    t0, %hi(_boot_page_table)
    li     t1, 0xffffffff80000000 - 0x80000000
    sub    t0, t0, t1
    srli   t0, t0, 12
    # Use Sv39 mode
    li     t1, 8 << 60
    or     t0, t0, t1
    csrw   satp, t0
    sfence.vma

    # a0 == hartid

    # set sp
    # sp = _bootstack + (hartid + 1) * (2^16)
    lui     sp, %hi(_bootstack)
    addi    sp, sp, %lo(_bootstack)
    addi    t0, a0, 1
    slli    t0, t0, 16
    add     sp, sp, t0

    # Jump to _boot (Absolute address)
    lui     t0, %hi(_boot)
    addi    t0, t0, %lo(_boot)
    jr      t0
    # j _boot

    .section .data
    .align 12   # page align
_boot_page_table:
    # sv39 mode
    .quad 0
    .quad 0
    # 0x00000000_80000000 -> 0x80000000 (1G)
    .quad (0x80000 << 10) | 0xcf # VRWXAD
    .zero 505 * 8
    # for virtio
    # 0xffffffff_00000000 -> 0x00000000 (1G)
    .quad (0x00000 << 10) | 0xcf
    .quad 0
     # 0xffffffff_80000000 -> 0x80000000 (1G)
    .quad (0x80000 << 10) | 0xcf # VRWXAD
    .quad 0
