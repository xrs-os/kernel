# XLENB 常量宏定义在 interrupt.rs 中

    .section .text
    .global _trap_entry
    .align 12
_trap_entry:

    # 从用户态进入中断: 
    #   sp       == 用户栈指针
    #   sscratch == 内核栈指针
    # 从内核态进入中断:
    #   sp       == 内核栈指针
    #   sscratch == 0

    # 交换 sp 和 sscratch,
    csrrw sp, sscratch, sp
    beqz sp, _store_kernel_context # sp == 0, 表示从内核进入中断

_store_user_trap_frame:
    sd x5, 15*XLENB(sp)
    ld x5, 14*XLENB(sp) # x5 = &Context

    sd x1, 2*XLENB(x5)
    sd x3, 4*XLENB(x5)
    sd x4, 5*XLENB(x5)
    sd x6, 7*XLENB(x5)
    sd x7, 8*XLENB(x5)
    sd x8, 9*XLENB(x5)
    sd x9, 10*XLENB(x5)
    sd x10, 11*XLENB(x5)
    sd x11, 12*XLENB(x5)
    sd x12, 13*XLENB(x5)
    sd x13, 14*XLENB(x5)
    sd x14, 15*XLENB(x5)
    sd x15, 16*XLENB(x5)
    sd x16, 17*XLENB(x5)
    sd x17, 18*XLENB(x5)
    sd x18, 19*XLENB(x5)
    sd x19, 20*XLENB(x5)
    sd x20, 21*XLENB(x5)
    sd x21, 22*XLENB(x5)
    sd x22, 23*XLENB(x5)
    sd x23, 24*XLENB(x5)
    sd x24, 25*XLENB(x5)
    sd x25, 26*XLENB(x5)
    sd x26, 27*XLENB(x5)
    sd x27, 28*XLENB(x5)
    sd x28, 29*XLENB(x5)
    sd x29, 30*XLENB(x5)
    sd x30, 31*XLENB(x5)
    sd x31, 32*XLENB(x5)
    
    ld x6, 15*XLENB(sp) # x6 = x5
    sd x6, 6*XLENB(x5)  # 保存 x5 寄存器到 Context

    csrrw t1, sscratch, x0 # 进入内核代码将 sscratch 置 0.
    sd t1, 3*XLENB(x5)   # 保存用户的 sp

    # 保存 sepc
    csrr t1, sepc
    sd t1, 0*XLENB(x5)

    # 保存 sstatus
    csrr t1, sstatus
    sd t1, 1*XLENB(x5)


    mv a0, x5
    call _user_trap_handler
_user_return:
    # -- 恢复 callee 寄存器--
    ld s0, 0*XLENB(sp)
    ld s1, 1*XLENB(sp)
    ld s2, 2*XLENB(sp)
    ld s3, 3*XLENB(sp)
    ld s4, 4*XLENB(sp)
    ld s5, 5*XLENB(sp)
    ld s6, 6*XLENB(sp)
    ld s7, 7*XLENB(sp)
    ld s8, 8*XLENB(sp)
    ld s9, 9*XLENB(sp)
    ld s10, 10*XLENB(sp)
    ld s11, 11*XLENB(sp)
    ld ra, 12*XLENB(sp)
    # ----------------------
    ld tp, 13*XLENB(sp) # 加载 硬件线程id
    addi sp, sp, 15 * XLENB
    ret

    .global _run_user
_run_user:
    addi sp, sp, -15 * XLENB
    # -- 保存 callee 寄存器 --
    sd s0, 0*XLENB(sp)
    sd s1, 1*XLENB(sp)
    sd s2, 2*XLENB(sp)
    sd s3, 3*XLENB(sp)
    sd s4, 4*XLENB(sp)
    sd s5, 5*XLENB(sp)
    sd s6, 6*XLENB(sp)
    sd s7, 7*XLENB(sp)
    sd s8, 8*XLENB(sp)
    sd s9, 9*XLENB(sp)
    sd s10, 10*XLENB(sp)
    sd s11, 11*XLENB(sp)
    sd ra, 12*XLENB(sp)
    # ----------------------
    sd tp, 13*XLENB(sp) # 保存 硬件线程id
    sd a0, 14*XLENB(sp) # 保存 Context 地址
    csrw sscratch, sp   # 将内核栈指针 sp 保存到 sscratch

_restore_user_context:
    # a0 == Context
    # 恢复 spec
    ld t0, 0*XLENB(a0)
    csrw sepc, t0
    
    # 恢复 sstatus
    ld t0, 1*XLENB(a0)
    csrw sstatus, t0

    ld x1, 2*XLENB(a0)
    ld sp, 3*XLENB(a0)
    ld x3, 4*XLENB(a0)
    ld x4, 5*XLENB(a0)
    ld x5, 6*XLENB(a0)
    ld x6, 7*XLENB(a0)
    ld x7, 8*XLENB(a0)
    ld x8, 9*XLENB(a0)
    ld x9, 10*XLENB(a0)
    ld x11, 12*XLENB(a0)
    ld x12, 13*XLENB(a0)
    ld x13, 14*XLENB(a0)
    ld x14, 15*XLENB(a0)
    ld x15, 16*XLENB(a0)
    ld x16, 17*XLENB(a0)
    ld x17, 18*XLENB(a0)
    ld x18, 19*XLENB(a0)
    ld x19, 20*XLENB(a0)
    ld x20, 21*XLENB(a0)
    ld x21, 22*XLENB(a0)
    ld x22, 23*XLENB(a0)
    ld x23, 24*XLENB(a0)
    ld x24, 25*XLENB(a0)
    ld x25, 26*XLENB(a0)
    ld x26, 27*XLENB(a0)
    ld x27, 28*XLENB(a0)
    ld x28, 29*XLENB(a0)
    ld x29, 30*XLENB(a0)
    ld x30, 31*XLENB(a0)
    ld x31, 32*XLENB(a0)
    ld x10, 11*XLENB(a0) # x10 == a0, 最后恢复
    sret




_store_kernel_context:
    csrr sp, sscratch # 还原内核栈指针
    addi sp, sp, -33 * XLENB

    sd x1, 2*XLENB(sp)
    sd x3, 4*XLENB(sp)
    sd x4, 5*XLENB(sp)
    sd x5, 6*XLENB(sp)
    sd x6, 7*XLENB(sp)
    sd x7, 8*XLENB(sp)
    sd x8, 9*XLENB(sp)
    sd x9, 10*XLENB(sp)
    sd x10, 11*XLENB(sp)
    sd x11, 12*XLENB(sp)
    sd x12, 13*XLENB(sp)
    sd x13, 14*XLENB(sp)
    sd x14, 15*XLENB(sp)
    sd x15, 16*XLENB(sp)
    sd x16, 17*XLENB(sp)
    sd x17, 18*XLENB(sp)
    sd x18, 19*XLENB(sp)
    sd x19, 20*XLENB(sp)
    sd x20, 21*XLENB(sp)
    sd x21, 22*XLENB(sp)
    sd x22, 23*XLENB(sp)
    sd x23, 24*XLENB(sp)
    sd x24, 25*XLENB(sp)
    sd x25, 26*XLENB(sp)
    sd x26, 27*XLENB(sp)
    sd x27, 28*XLENB(sp)
    sd x28, 29*XLENB(sp)
    sd x29, 30*XLENB(sp)
    sd x30, 31*XLENB(sp)
    sd x31, 32*XLENB(sp)
    
    csrrw t0, sscratch, x0 # 进入内核代码将 sscratch 置 0.
    sd t0, 3*XLENB(sp)     # 保存中断前的 sp

    # 保存 sepc
    csrr t0, sepc
    sd t0, 0*XLENB(sp)

    # 保存 sstatus
    csrr t1, sstatus
    sd t1, 1*XLENB(sp)


    mv a0, sp
    call _kernel_trap_handler

_restore_kernel_trap_frame:
   
    # 恢复 spec
    ld t0, 0*XLENB(sp)
    csrw sepc, t0
    
    # 恢复 sstatus
    ld t0, 1*XLENB(sp)
    csrw sstatus, t0

    ld x1, 2*XLENB(sp)
    ld x3, 4*XLENB(sp)
    ld x4, 5*XLENB(sp)
    ld x5, 6*XLENB(sp)
    ld x6, 7*XLENB(sp)
    ld x7, 8*XLENB(sp)
    ld x8, 9*XLENB(sp)
    ld x9, 10*XLENB(sp)
    ld x10, 11*XLENB(sp)
    ld x11, 12*XLENB(sp)
    ld x12, 13*XLENB(sp)
    ld x13, 14*XLENB(sp)
    ld x14, 15*XLENB(sp)
    ld x15, 16*XLENB(sp)
    ld x16, 17*XLENB(sp)
    ld x17, 18*XLENB(sp)
    ld x18, 19*XLENB(sp)
    ld x19, 20*XLENB(sp)
    ld x20, 21*XLENB(sp)
    ld x21, 22*XLENB(sp)
    ld x22, 23*XLENB(sp)
    ld x23, 24*XLENB(sp)
    ld x24, 25*XLENB(sp)
    ld x25, 26*XLENB(sp)
    ld x26, 27*XLENB(sp)
    ld x27, 28*XLENB(sp)
    ld x28, 29*XLENB(sp)
    ld x29, 30*XLENB(sp)
    ld x30, 31*XLENB(sp)
    ld x31, 32*XLENB(sp)
    
    ld sp, 3*XLENB(sp) # 最后恢复 sp
    
    sret