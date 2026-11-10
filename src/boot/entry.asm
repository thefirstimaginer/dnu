; entry.asm - Assembly entry point for the DNU kernel
; Transfers control to the Rust entry point `_start_rust`.

global _start
extern _start_rust

section .text
_start:
    ; Disable interrupts immediately
    cli

    ; Call the Rust entry point (uses Limine's stack via StackSizeRequest)
    call _start_rust

    ; Should never return
    cli
    hlt

section .data
align 16
stack_bottom:
    resb 1048576  ; 1 MiB stack (backup)
stack_top:
