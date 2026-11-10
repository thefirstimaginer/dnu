use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::mem::size_of;

global_asm!(
    r#"
.global dreamcore_exception_0
dreamcore_exception_0: push 0; push 0; jmp dreamcore_exception_common
.global dreamcore_exception_1
dreamcore_exception_1: push 0; push 1; jmp dreamcore_exception_common
.global dreamcore_exception_2
dreamcore_exception_2: push 0; push 2; jmp dreamcore_exception_common
.global dreamcore_exception_3
dreamcore_exception_3: push 0; push 3; jmp dreamcore_exception_common
.global dreamcore_exception_4
dreamcore_exception_4: push 0; push 4; jmp dreamcore_exception_common
.global dreamcore_exception_5
dreamcore_exception_5: push 0; push 5; jmp dreamcore_exception_common
.global dreamcore_exception_6
dreamcore_exception_6: push 0; push 6; jmp dreamcore_exception_common
.global dreamcore_exception_7
dreamcore_exception_7: push 0; push 7; jmp dreamcore_exception_common
.global dreamcore_exception_8
dreamcore_exception_8: push 8; jmp dreamcore_exception_common
.global dreamcore_exception_9
dreamcore_exception_9: push 0; push 9; jmp dreamcore_exception_common
.global dreamcore_exception_10
dreamcore_exception_10: push 10; jmp dreamcore_exception_common
.global dreamcore_exception_11
dreamcore_exception_11: push 11; jmp dreamcore_exception_common
.global dreamcore_exception_12
dreamcore_exception_12: push 12; jmp dreamcore_exception_common
.global dreamcore_exception_13
dreamcore_exception_13: push 13; jmp dreamcore_exception_common
.global dreamcore_exception_14
dreamcore_exception_14: push 14; jmp dreamcore_exception_common
.global dreamcore_exception_15
dreamcore_exception_15: push 0; push 15; jmp dreamcore_exception_common
.global dreamcore_exception_16
dreamcore_exception_16: push 0; push 16; jmp dreamcore_exception_common
.global dreamcore_exception_17
dreamcore_exception_17: push 17; jmp dreamcore_exception_common
.global dreamcore_exception_18
dreamcore_exception_18: push 0; push 18; jmp dreamcore_exception_common
.global dreamcore_exception_19
dreamcore_exception_19: push 0; push 19; jmp dreamcore_exception_common
.global dreamcore_exception_20
dreamcore_exception_20: push 0; push 20; jmp dreamcore_exception_common
.global dreamcore_exception_21
dreamcore_exception_21: push 21; jmp dreamcore_exception_common
.global dreamcore_exception_22
dreamcore_exception_22: push 0; push 22; jmp dreamcore_exception_common
.global dreamcore_exception_23
dreamcore_exception_23: push 0; push 23; jmp dreamcore_exception_common
.global dreamcore_exception_24
dreamcore_exception_24: push 0; push 24; jmp dreamcore_exception_common
.global dreamcore_exception_25
dreamcore_exception_25: push 0; push 25; jmp dreamcore_exception_common
.global dreamcore_exception_26
dreamcore_exception_26: push 0; push 26; jmp dreamcore_exception_common
.global dreamcore_exception_27
dreamcore_exception_27: push 0; push 27; jmp dreamcore_exception_common
.global dreamcore_exception_28
dreamcore_exception_28: push 0; push 28; jmp dreamcore_exception_common
.global dreamcore_exception_29
dreamcore_exception_29: push 29; jmp dreamcore_exception_common
.global dreamcore_exception_30
dreamcore_exception_30: push 30; jmp dreamcore_exception_common
.global dreamcore_exception_31
dreamcore_exception_31: push 0; push 31; jmp dreamcore_exception_common

dreamcore_exception_common:
    cli
    mov rdi, [rsp]
    mov rsi, [rsp + 8]
    mov rdx, [rsp + 16]
    mov rcx, [rsp + 24]
    mov r8, cr2
    and rsp, -16
    call dreamcore_exception_handler
1:  hlt
    jmp 1b

.global dreamcore_syscall_stub
dreamcore_syscall_stub:
    // Diagnóstico direto via UART para confirmar entrada no stub de syscall.
    mov al, 'S'
    mov dx, 0x3f8
    out dx, al
    mov al, 10
    out dx, al
    jmp .continue_stub
.continue_stub:
    // O hardware salva RIP do userspace em RCX e RFLAGS em R11 durante SYSCALL.
    // Devemos preservar RCX e R11 no TrapFrame para que SYSRETQ restaure
    // corretamente o RIP e RFLAGS do userspace ao retornar.
    // Ordem dos pushes deve coincidir exatamente com o layout de TrapFrame
    // (de baixo para cima: rax, rbx, rcx, rdx, rsi, rdi, rbp, r8, r9, r10,
    // r11, r12, r13, r14, r15 — o último push fica no topo da pilha).
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rdi, rsp
    and rsp, -16
    call dreamcore_syscall_dispatch
    mov rsp, rax
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    sysretq

.global dreamcore_timer_interrupt
dreamcore_timer_interrupt:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rdi, rsp
    and rsp, -16
    call dreamcore_timer_dispatch
    mov rsp, rax
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq

.global dreamcore_resume_trap_frame
dreamcore_resume_trap_frame:
    mov rsp, rdi
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq
"#
);

unsafe extern "C" {
    fn dreamcore_exception_0();
    fn dreamcore_exception_1();
    fn dreamcore_exception_2();
    fn dreamcore_exception_3();
    fn dreamcore_exception_4();
    fn dreamcore_exception_5();
    fn dreamcore_exception_6();
    fn dreamcore_exception_7();
    fn dreamcore_exception_8();
    fn dreamcore_exception_9();
    fn dreamcore_exception_10();
    fn dreamcore_exception_11();
    fn dreamcore_exception_12();
    fn dreamcore_exception_13();
    fn dreamcore_exception_14();
    fn dreamcore_exception_15();
    fn dreamcore_exception_16();
    fn dreamcore_exception_17();
    fn dreamcore_exception_18();
    fn dreamcore_exception_19();
    fn dreamcore_exception_20();
    fn dreamcore_exception_21();
    fn dreamcore_exception_22();
    fn dreamcore_exception_23();
    fn dreamcore_exception_24();
    fn dreamcore_exception_25();
    fn dreamcore_exception_26();
    fn dreamcore_exception_27();
    fn dreamcore_exception_28();
    fn dreamcore_exception_29();
    fn dreamcore_exception_30();
    fn dreamcore_exception_31();
    fn dreamcore_syscall_stub();
    fn dreamcore_timer_interrupt();
    fn dreamcore_resume_trap_frame(frame: *mut crate::abi::trap::TrapFrame) -> !;
}

/// Entry point for the periodic PIT interrupt (IRQ0). It preserves the interrupted
/// trap frame, hands control to the scheduler for possible preemption, then returns
/// the frame to resume on top of the iretq instruction.
#[unsafe(no_mangle)]
extern "C" fn dreamcore_timer_dispatch(frame: *mut crate::abi::trap::TrapFrame) -> *mut crate::abi::trap::TrapFrame {
    crate::arch_hub::x86_64::timer::handle_interrupt(frame)
}


#[unsafe(no_mangle)]
extern "C" fn dreamcore_exception_handler(
    vector: u64,
    error: u64,
    rip: u64,
    cs: u64,
    cr2: u64,
) -> ! {
    if cs & 3 == 3 {
        // `#PF` de escrita em uma VMA de `mmap` que ainda não foi populada: em
        // vez de matar o processo, instala a página zerada e retoma a instrução
        // que falhou. `bits 0..2` do código de erro são `P` (presente), `W/R`
        // (escrita) e `U/S` (usuário) — só tratamos escrita (bit 1).
        if vector == 14 && error & 0b010 != 0 && crate::process::handle_mmap_fault(cr2) {
            // Retoma a instrução que faltou (`iretq` para o mesmo `rip`).
            let frame = crate::process::current_trap_frame();
            unsafe {
                dreamcore_resume_trap_frame(frame);
            }
        }
        crate::serial_log_timestamp();
        serial_write(b"User process fault; terminating PID ");
        serial_hex(crate::process::current_pid() as u64);
        serial_write(b" vector=0x");
        serial_hex(vector);
        serial_write(b" error=0x");
        serial_hex(error);
        serial_write(b" rip=0x");
        serial_hex(rip);
        serial_write(b" cr2=0x");
        serial_hex(cr2);
        serial_write(b"\r\n");
        let next = crate::process::terminate_faulting_process();
        unsafe {
            dreamcore_resume_trap_frame(next);
        }
    }
    crate::serial_log_timestamp();
    serial_write(b"FATAL EXCEPTION vector=0x");
    serial_hex(vector);
    serial_write(b" error=0x");
    serial_hex(error);
    // Decode page fault error code bits:
    //   bit 0 (P): 0=not-present, 1=protection violation
    //   bit 1 (W/R): 0=read, 1=write
    //   bit 2 (U/S): 0=supervisor, 1=user
    //   bit 3 (RSVD): reserved bit violation
    //   bit 4 (ID): instruction fetch
    if vector == 14 {
        serial_write(b" [PF:P=");
        serial_write(if error & 1 != 0 { b"1" } else { b"0" });
        serial_write(b",W=");
        serial_write(if error & 2 != 0 { b"1" } else { b"0" });
        serial_write(b",U=");
        serial_write(if error & 4 != 0 { b"1" } else { b"0" });
        serial_write(b",R=");
        serial_write(if error & 8 != 0 { b"1" } else { b"0" });
        serial_write(b",I=");
        serial_write(if error & 16 != 0 { b"1" } else { b"0" });
        serial_write(b"]");
    }
    serial_write(b" rip=0x");
    serial_hex(rip);
    serial_write(b" cs=0x");
    serial_hex(cs);
    serial_write(b" cr2=0x");
    serial_hex(cr2);
    serial_write(b"\r\n");
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const MISSING: Self = Self {
        offset_low: 0,
        selector: 0,
        ist: 0,
        attributes: 0,
        offset_middle: 0,
        offset_high: 0,
        reserved: 0,
    };

    fn interrupt_gate(handler: u64, selector: u16) -> Self {
        Self {
            offset_low: handler as u16,
            selector,
            ist: 0,
            attributes: 0x8e,
            offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

struct SharedIdt(UnsafeCell<[IdtEntry; 256]>);
unsafe impl Sync for SharedIdt {}

/// IDT colocada em `.data` para garantir que o Limine alocar páginas para ela.
#[unsafe(link_section = ".data")]
#[unsafe(link_section = ".data.early")]
static IDT: SharedIdt = SharedIdt(UnsafeCell::new([IdtEntry::MISSING; 256]));

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

pub fn init() {
    serial_write(b"step: idt init start\r\n");
    let idt = unsafe { &mut *IDT.0.get() };
    let selector: u16;
    unsafe {
        asm!("mov {0:x}, cs", out(reg) selector, options(nomem, nostack, preserves_flags));
    }
    serial_write(b"step: idt cs read ok\r\n");
    let handlers = [
        dreamcore_exception_0,
        dreamcore_exception_1,
        dreamcore_exception_2,
        dreamcore_exception_3,
        dreamcore_exception_4,
        dreamcore_exception_5,
        dreamcore_exception_6,
        dreamcore_exception_7,
        dreamcore_exception_8,
        dreamcore_exception_9,
        dreamcore_exception_10,
        dreamcore_exception_11,
        dreamcore_exception_12,
        dreamcore_exception_13,
        dreamcore_exception_14,
        dreamcore_exception_15,
        dreamcore_exception_16,
        dreamcore_exception_17,
        dreamcore_exception_18,
        dreamcore_exception_19,
        dreamcore_exception_20,
        dreamcore_exception_21,
        dreamcore_exception_22,
        dreamcore_exception_23,
        dreamcore_exception_24,
        dreamcore_exception_25,
        dreamcore_exception_26,
        dreamcore_exception_27,
        dreamcore_exception_28,
        dreamcore_exception_29,
        dreamcore_exception_30,
        dreamcore_exception_31,
    ];
    for (entry, handler) in idt.iter_mut().zip(handlers) {
        let handler = handler as *const () as usize as u64;
        *entry = IdtEntry::interrupt_gate(handler, selector);
    }
    serial_write(b"step: idt entries filled ok\r\n");
    let timer_handler = dreamcore_timer_interrupt as *const () as usize as u64;
    idt[crate::time::timer_vector() as usize] =
        IdtEntry::interrupt_gate(timer_handler, selector);
    serial_write(b"step: idt timer set ok\r\n");
    let pointer = DescriptorTablePointer {
        limit: (size_of::<[IdtEntry; 256]>() - 1) as u16,
        base: idt.as_ptr() as u64,
    };
    serial_write(b"step: idt pointer built ok\r\n");
    unsafe {
        asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
    }
    serial_write(b"step: idt lidt done ok\r\n");
    crate::subsystems::mark_ready(crate::subsystems::Subsystem::DescriptorTables);
}

/// Lê um MSR e retorna seu valor de 64 bits.
unsafe fn rdmsr(msr: u32) -> u64 {
    let eax: u32;
    let edx: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") eax,
        out("edx") edx,
        options(nostack)
    );
    ((edx as u64) << 32) | (eax as u64)
}

/// Configura MSRs para suporte à instrução `syscall` em modo longo.
///
/// O kernel define o endereço do handler de syscall em IA32_LSTAR e configura
/// os seletores de segmento em IA32_STAR. Após isso, processos em ring 3 podem
/// usar `syscall` para invocar syscalls do kernel.
pub fn configure_syscall_msrs() {
    let syscall_stub = dreamcore_syscall_stub as *const () as usize as u64;
    serial_write(b"step: syscall MSRs start\r\n");

    // IA32_EFER = habilitar SCE (System Call Enable) bit 0 e NXE bit 11
    // para usar SYSCALL/SYSRET e permitir o bit NX (63) nas entradas de
    // tabela de páginas.
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xC0000080u32,
            in("eax") 0x801u32,
            in("edx") 0u32,
            options(nostack)
        );
    }
    serial_write(b"step: syscall EFER ok\r\n");

    // IA32_LSTAR = endereço do handler de syscall
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xC0000082u32,
            in("eax") syscall_stub as u32,
            in("edx") (syscall_stub >> 32) as u32,
            options(nostack)
        );
    }
    serial_write(b"step: syscall LSTAR ok\r\n");
    crate::serial_write(b"  LSTAR=0x");
    let lstar = unsafe { rdmsr(0xC0000082) };
    crate::serial_write_hex(lstar);
    crate::serial_write(b"\r\n");

    // IA32_STAR = segmento CS (bits 47-32 = usuário menos 0x10) e SS (bits 31-0 = kernel e usuário).
    // syscall adiciona 0x10 automaticamente ao STAR[47:32] para formar o CS do modo usuário.
    // USER_CS = 0x2b, então STAR[47:32] = 0x2b - 0x10 = 0x1b.
    // KERNEL_SS = 0x10, USER_SS = 0x33.
    // Bits 63:48 são reservados (devem ser 0).
    let star: u64 = ((0x00u64) << 48) | ((0x1bu64) << 32) | ((0x10u64) << 16) | (0x33u64);
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xC0000081u32,
            in("eax") star as u32,
            in("edx") (star >> 32) as u32,
            options(nostack)
        );
    }
    serial_write(b"step: syscall STAR ok\r\n");

    // IA32_FMASK = flags removidas ao entrar em ring 0 (0 = não limpa nada, seguro).
    // Pulado: wrmsr(0xC0000084) causa reset neste QEMU; FMASK=0 é o valor padrão.

    serial_write(b"syscall MSRs configured\r\n");
}

fn serial_write(bytes: &[u8]) {
    let mut previous_was_cr = false;
    for &byte in bytes {
        if byte == b'\n' && !previous_was_cr {
            serial_write_byte(b'\r');
        }
        serial_write_byte(byte);
        previous_was_cr = byte == b'\r';
    }
}

fn serial_write_byte(byte: u8) {
    unsafe {
        while in_port(0x3fd) & 0x20 == 0 {
            asm!("pause", options(nomem, nostack, preserves_flags));
        }
        asm!("out dx, al", in("dx") 0x3f8u16, in("al") byte, options(nomem, nostack, preserves_flags));
    }
}

fn serial_hex(mut value: u64) {
    let mut digits = [0u8; 16];
    for index in (0..digits.len()).rev() {
        let digit = (value & 0xf) as u8;
        digits[index] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        value >>= 4;
    }
    serial_write(&digits);
}

unsafe fn in_port(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    value
}
