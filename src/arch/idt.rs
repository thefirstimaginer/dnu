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
    // O hardware salva RIP do userspace em RCX e RFLAGS em R11 durante SYSCALL.
    // RSP ainda é o do userspace (syscall não faz stack switch). Não podemos
    // usar r10 para salvar o user_rsp porque r10 é o 4º argumento do usuário
    // (syscall usa r10, não rcx). Usamos r11 (já clobberado pelo hardware,
    // contendo RFLAGS) para guardar user_rsp temporariamente.
    mov r11, rsp
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
    // rsp agora aponta para o início do TrapFrame (r15 em rsp+0).
    // Salva o RSP do userspace (em r11) no campo rsp do frame (offset 144).
    mov [rsp + 144], r11
    mov rdi, rsp            // rdi = ponteiro para o TrapFrame (frame base)
    and rsp, -16            // Alinha rsp para a chamada Rust (ABI exige 16-byte align antes do call)
    // rsp pode ter sido movido pelo and acima, então usamos rdi (frame base) para acessar
    // os campos do TrapFrame em offsets absolutos, não relativos ao rsp alinhado.
    mov rax, [rdi + 96]     // rax = rcx salvo (offset 96 no TrapFrame)
    mov [rdi + 120], rax    // rip = rcx (offset 120 no TrapFrame)
    push rdi                // salva frame pointer na pilha (rdi clobbered pelo call)
    call dreamcore_syscall_dispatch  // rax = próximo frame
    pop rdi                 // restaura frame pointer
    // Ler RIP/CS/RFLAGS/RSP/SS diretamente do frame via rdi (restaurado).
    mov r10, [rdi + 120]   // rip
    mov r11, [rdi + 128]   // cs
    mov r12, [rdi + 136]   // rflags
    mov r13, [rdi + 144]   // rsp
    mov r14, [rdi + 152]   // ss
    // rsp agora aponta para a stack do usuário.
    mov rsp, r13
    // Escreve RIP/CS/RFLAGS/RSP/SS na pilha onde o iretq lê.
    mov [rsp + 0], r10     // RIP
    mov [rsp + 8], r11     // CS
    mov [rsp + 16], r12    // RFLAGS
    mov [rsp + 24], r13    // RSP
    mov [rsp + 32], r14    // SS
    // Usa iretq para retornar para ring 3.
    iretq

.global dreamcore_int80_stub
dreamcore_int80_stub:
    // Diagnóstico: confirma entrada no handler de int 0x80.
    mov al, '8'
    mov dx, 0x3f8
    out dx, al
    mov al, 10
    out dx, al
    // int 0x80 já salvou [RIP, CS, RFLAGS, RSP, SS] na pilha do usuário.
    // Salvamos os registradores gerais na ordem do TrapFrame, depois chamamos
    // o dispatcher e retornamos com iretq (que lê RIP/CS/RFLAGS/RSP/SS da
    // pilha — precisamente os campos do TrapFrame em offsets 120..152).
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
    add rsp, 8        // Compensa os 5 itens (RIP,CS,RFLAGS,RSP,SS) salvos por int 0x80
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
    // rsp agora aponta para o campo rip (offset 120) do TrapFrame. iretq
    // carrega RIP, CS, RFLAGS, RSP, SS da pilha.
    iretq

.global dreamcore_timer_interrupt
dreamcore_timer_interrupt:
    // Salva o RSP original antes de qualquer push: necessário para retornar
    // corretamente para ring 0 quando o timer ocorre durante uma syscall
    // (o hardware não empilha SS/RSP sem change de ring).
    mov r10, rsp
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
    // Após 16 pops (r15..rax), rsp avança 128 bytes e aponta para o campo
    // cs (offset 120 do TrapFrame). Para o iretq, precisamos que rsp aponte
    // para o campo rip (offset 120), então fazemos sub rsp, 8.
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
    sub rsp, 8          // rsp agora aponta para o campo rip (offset 120)
    // rsp+0 = rip, rsp+8 = cs, rsp+16 = rflags, rsp+24 = rsp, rsp+32 = ss
    mov ax, [rsp + 8]   // carrega cs do frame
    and ax, 3           // cs & 3 == 3? (userspace?)
    jz .timer_iret_kernel
    jmp .timer_iret_userspace
.timer_iret_kernel:
    // Veio de ring 0: o hardware não empilhou SS/RSP. Restaura RSP original
    // (salvo em r10) e SS kernel para o iretq retornar corretamente para ring 0.
    mov rax, r10
    mov [rsp + 24], rax           // RSP = kernel stack no momento da interrupção
    mov ax, 0x10
    mov [rsp + 32], ax            // SS = KERNEL_DATA_SELECTOR
    mov ax, 0x08
    mov [rsp + 8], ax             // CS = KERNEL_CODE_SELECTOR
    mov eax, 0x202
    mov [rsp + 16], eax           // RFLAGS = IF=1
    jmp .timer_iret_done
.timer_iret_userspace:
    // Garante que CS/SS/RFLAGS são válidos para ring 3 antes do iretq.
    // CS = USER_CODE_SELECTOR (0x2b), SS = USER_DATA_SELECTOR (0x33).
    mov ax, 0x2b
    mov [rsp + 8], ax             // CS = usuário
    mov ax, 0x33
    mov [rsp + 32], ax            // SS = usuário
    mov eax, 0x202
    mov [rsp + 16], eax           // RFLAGS = IF=1
    jmp .timer_iret_done
.timer_iret_done:
    // rsp já aponta para o campo rip (offset 120). O iretq carrega RIP, CS,
    // RFLAGS, RSP, SS da pilha.
    iretq

.print_hex16:
    // Imprime AX em hex na UART (4 dígitos).
    push rcx
    mov rcx, 12
.print_hex_loop:
    rol ax, 4
    mov rbx, rax
    and ebx, 0xF
    cmp ebx, 9
    jbe .print_hex_digit
    add ebx, 7  // 'A' - 10
.print_hex_digit:
    add ebx, 0x30  // '0'
    mov dx, 0x3f8
    mov al, bl
    out dx, al
    loop .print_hex_loop
    pop rcx
    ret

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
    fn dreamcore_int80_stub();
    fn dreamcore_timer_interrupt();
    fn dreamcore_resume_trap_frame(frame: *mut crate::abi::trap::TrapFrame) -> !;
}

/// Valores salvos do próximo TrapFrame antes do `paging::switch()` no
/// `schedule()`. São restaurados pelo assembly do `dreamcore_syscall_stub`
/// antes do `iretq`, garantindo que o CR3 switch não apague o RIP/CS/etc.
/// do frame que será restaurado em ring 3.
static mut NEXT_FRAME_RIP: u64 = 0;
static mut NEXT_FRAME_CS: u64 = 0;
static mut NEXT_FRAME_RFLAGS: u64 = 0;
static mut NEXT_FRAME_RSP: u64 = 0;
static mut NEXT_FRAME_SS: u64 = 0;
static mut NEXT_FRAME_VALID: bool = false;

/// Funções de acesso para ler os valores salvos do próximo frame.
/// Chamadas pelo assembly do syscall stub antes do iretq.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dreamcore_next_rip() -> u64 { NEXT_FRAME_RIP }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dreamcore_next_cs() -> u64 { NEXT_FRAME_CS }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dreamcore_next_rflags() -> u64 { NEXT_FRAME_RFLAGS }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dreamcore_next_rsp() -> u64 { NEXT_FRAME_RSP }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dreamcore_next_ss() -> u64 { NEXT_FRAME_SS }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dreamcore_next_valid() -> u8 { if NEXT_FRAME_VALID { 1 } else { 0 } }

/// Carrega todos os valores do próximo frame em um bloco de 5 u64 (RIP, CS,
/// RFLAGS, RSP, SS) apontado por `dst`. Chamada pelo assembly antes do iretq.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dreamcore_next_values(dst: *mut u64) {
    if dst.is_null() { return; }
    unsafe {
        *dst.add(0) = NEXT_FRAME_RIP;
        *dst.add(1) = NEXT_FRAME_CS;
        *dst.add(2) = NEXT_FRAME_RFLAGS;
        *dst.add(3) = NEXT_FRAME_RSP;
        *dst.add(4) = NEXT_FRAME_SS;
    }
}

/// Quando `schedule()` precisa mudar de address space (context switch), ele
/// salva o novo CR3 aqui. O assembly chama `dreamcore_get_next_cr3()` e faz o
/// `mov cr3` antes de acessar a stack do usuário para o `iretq`.
static mut NEXT_CR3: u64 = 0;
static mut NEXT_CR3_VALID: bool = false;

pub(crate) static mut NEXT_CR3_PUB: u64 = 0;
pub(crate) static mut NEXT_CR3_VALID_PUB: bool = false;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dreamcore_get_next_cr3() -> u64 {
    if NEXT_CR3_VALID_PUB {
        NEXT_CR3_VALID_PUB = false;
        NEXT_CR3_PUB
    } else {
        0
    }
}

/// Entry point for the periodic PIT interrupt (IRQ0). It preserves the interrupted
/// trap frame, hands control to the scheduler for possible preemption, then returns
/// the frame to resume on top of the iretq instruction.
#[unsafe(no_mangle)]
extern "C" fn dreamcore_timer_dispatch(frame: *mut crate::abi::trap::TrapFrame) -> *mut crate::abi::trap::TrapFrame {
    let next = crate::arch_hub::x86_64::timer::handle_interrupt(frame);
    // Garante que os seletores de segmento e flags são válidos para ring 3
    // antes do iretq. Se o retorno é para userspace (cs&3==3), corrige CS/SS/
    // RFLAGS caso o scheduler tenha deixado valores inconsistentes.
    if !next.is_null() && unsafe { (*next).cs & 3 == 3 } {
        unsafe {
            (*next).cs = crate::gdt::USER_CODE_SELECTOR;
            (*next).ss = crate::gdt::USER_DATA_SELECTOR;
            (*next).rflags = 0x202; // IF=1, bit 1 (reservado) sempre 1
        }
        crate::serial_write(b"timer_iret: cs=0x");
        crate::serial_write_hex(crate::gdt::USER_CODE_SELECTOR);
        crate::serial_write(b" ss=0x");
        crate::serial_write_hex(crate::gdt::USER_DATA_SELECTOR);
        crate::serial_write(b" rflags=0x202 rip=0x");
        crate::serial_write_hex(unsafe { (*next).rip });
        crate::serial_write(b" rsp=0x");
        crate::serial_write_hex(unsafe { (*next).rsp });
        crate::serial_write(b"\r\n");
    }
    next
}


/// Handler de exceções e page faults. Chamado pelo stub de assembly
/// `dreamcore_exception_common` com `(vector, error, rip, cs, cr2)`.
///
/// Para exceções de userspace (CS & 3 == 3), imprime diagnóstico detalhado e
/// termina o processo falho (ou, para o PID 1, tenta terminar graciosamente
/// sem travar o sistema). Para exceções de kernel, entra em loop `hlt`.
#[unsafe(no_mangle)]
extern "C" fn dreamcore_exception_handler(
    vector: u64,
    error: u64,
    rip: u64,
    cs: u64,
    cr2: u64,
) -> ! {
    let pid = crate::process::current_pid();
    let is_user = cs & 3 == 3;

    if is_user {
        // Obtém o frame do processo corrente para diagnóstico e possível retomada.
        let frame = crate::process::current_trap_frame();

        // `#PF` de escrita em uma VMA de `mmap` que ainda não foi populada: em
        // vez de matar o processo, instala a página zerada e retoma a instrução
        // que falhou. `bits 0..2` do código de erro são `P` (presente), `W/R`
        // (escrita) e `U/S` (usuário) — só tratamos escrita (bit 1).
        if vector == 14 && error & 0b010 != 0 && crate::process::handle_mmap_fault(cr2) {
            // Retoma a instrução que faltou (`iretq` para o mesmo `rip`).
            unsafe {
                dreamcore_resume_trap_frame(frame);
            }
        }

        // Exceções em userspace nunca pulam instruções nem retomam a execução
        // após uma falha de instrução ou de proteção geral. O processo é
        // terminado com sinal apropriado pelo fluxo abaixo.

        crate::serial_log_timestamp();
        serial_write(b"User process fault; terminating PID ");
        serial_hex(pid as u64);
        serial_write(b" vector=0x");
        serial_hex(vector);
        serial_write(b" error=0x");
        serial_hex(error);
        serial_write(b" rip=0x");
        serial_hex(rip);
        serial_write(b" cs=0x");
        serial_hex(cs);
        serial_write(b" rflags=0x");
        serial_hex(unsafe { if !frame.is_null() { (*frame).rflags } else { 0 } });
        serial_write(b" cr2=0x");
        serial_hex(cr2);
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
            serial_write(b"] user=");
            serial_write(if error & 4 != 0 { b"1" } else { b"0" });
        }
        serial_write(b"\r\n");

        // O PID 1 (init) é o supervisor do sistema: terminá-lo deixaria a
        // máquina sem quem lance ws_server, shell e explorer. Tenta terminar
        // graciosamente e deixar o escalonador escolher o próximo processo.
        if pid == 1 {
            serial_write(b"init: faulted, terminating gracefully\r\n");
            let next = crate::process::terminate_faulting_process();
            unsafe {
                dreamcore_resume_trap_frame(next);
            }
        }

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
    if vector == 13 {
        // Decode general protection fault error code:
        //   bit 0: 0=segment register, 1=IDT/descriptor table
        //   bits 1-15: segment selector index (if bit 0 = 0)
        serial_write(b" [GP:ext=");
        serial_write(if error & 1 != 0 { b"1" } else { b"0" });
        serial_write(b",sel=");
        crate::serial_write_hex((error >> 1) & 0x1fff);
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

#[unsafe(no_mangle)]
extern "C" fn dreamcore_page_fault_handler(
    frame: *mut crate::abi::trap::TrapFrame,
    error_code: u64,
) {
    let cr2: u64;
    unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)); }
    serial_write(b"#PF: cr2=0x");
    let mut buf = [0u8; 18];
    let mut n = 0;
    let mut v = cr2;
    if v == 0 { buf[n] = b'0'; n += 1; }
    while v > 0 { buf[n] = b'0' + (v % 16) as u8; if buf[n] > b'9' { buf[n] += 7; } n += 1; v /= 16; }
    for i in (0..n).rev() { serial_write(&buf[i..i+1]); }
    serial_write(b" err=0x");
    let mut n = 0;
    let mut v = error_code;
    if v == 0 { buf[n] = b'0'; n += 1; }
    while v > 0 { buf[n] = b'0' + (v % 16) as u8; if buf[n] > b'9' { buf[n] += 7; } n += 1; v /= 16; }
    for i in (0..n).rev() { serial_write(&buf[i..i+1]); }
    serial_write(b"\r\n");
    loop { unsafe { core::arch::asm!("hlt", options(nomem, nostack)); } }
}

#[unsafe(no_mangle)]
extern "C" fn dreamcore_gp_handler(
    _frame: *mut crate::abi::trap::TrapFrame,
    error_code: u64,
) {
    serial_write(b"#GP: err=0x");
    let mut buf = [0u8; 18];
    let mut n = 0;
    let mut v = error_code;
    if v == 0 { buf[n] = b'0'; n += 1; }
    while v > 0 { buf[n] = b'0' + (v % 16) as u8; if buf[n] > b'9' { buf[n] += 7; } n += 1; v /= 16; }
    for i in (0..n).rev() { serial_write(&buf[i..i+1]); }
    serial_write(b"\r\n");
    loop { unsafe { core::arch::asm!("hlt", options(nomem, nostack)); } }
}

/// Salva RIP/CS/RFLAGS/RSP/SS do próximo TrapFrame em variáveis estáticas,
/// ANTES de `paging::switch()` mudar o CR3. O assembly do syscall stub usa
/// esses valores para restaurar a pilha antes do `iretq`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dreamcore_save_next_frame(frame: *mut crate::abi::trap::TrapFrame) {
    if frame.is_null() {
        NEXT_FRAME_VALID = false;
        return;
    }
    unsafe {
        NEXT_FRAME_RIP = (*frame).rip;
        NEXT_FRAME_CS = (*frame).cs;
        NEXT_FRAME_RFLAGS = (*frame).rflags;
        NEXT_FRAME_RSP = (*frame).rsp;
        NEXT_FRAME_SS = (*frame).ss;
        NEXT_FRAME_VALID = true;
    }
    serial_write(b"save_next_frame: rip=0x");
    crate::serial_write_hex(NEXT_FRAME_RIP);
    serial_write(b" cs=0x");
    crate::serial_write_hex(NEXT_FRAME_CS);
    serial_write(b" rsp=0x");
    crate::serial_write_hex(NEXT_FRAME_RSP);
    serial_write(b"\r\n");
}

/// Restaura RIP/CS/RFLAGS/RSP/SS nas variáveis estáticas de volta para o
/// TrapFrame, DEPOIS de `paging::switch()`. Chamado pelo assembly do syscall
/// stub antes do `iretq`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dreamcore_restore_next_frame(frame: *mut crate::abi::trap::TrapFrame) {
    if frame.is_null() || !NEXT_FRAME_VALID {
        serial_write(b"restore_next_frame: INVALID or null\r\n");
        return;
    }
    serial_write(b"restore_next_frame: rip=0x");
    crate::serial_write_hex(NEXT_FRAME_RIP);
    serial_write(b" cs=0x");
    crate::serial_write_hex(NEXT_FRAME_CS);
    serial_write(b" rsp=0x");
    crate::serial_write_hex(NEXT_FRAME_RSP);
    serial_write(b"\r\n");
    unsafe {
        (*frame).rip = NEXT_FRAME_RIP;
        (*frame).cs = NEXT_FRAME_CS;
        (*frame).rflags = NEXT_FRAME_RFLAGS;
        (*frame).rsp = NEXT_FRAME_RSP;
        (*frame).ss = NEXT_FRAME_SS;
    }
    // Verifica se a escrita foi efetiva
    serial_write(b"restore_next_frame: after write rip=0x");
    crate::serial_write_hex(unsafe { (*frame).rip });
    serial_write(b"\r\n");
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
    // Registra o handler de syscall via int 0x80 (vetor 0x80 = 128).
    // DPL=3 (attributes 0xee) para permitir chamada de userspace (ring 3).
    let int80_handler = dreamcore_int80_stub as *const () as usize as u64;
    idt[0x80] = IdtEntry {
        offset_low: int80_handler as u16,
        selector: selector,
        ist: 0,
        attributes: 0xee, // Present=1, DPL=3, system, 32-bit interrupt gate
        offset_middle: (int80_handler >> 16) as u16,
        offset_high: (int80_handler >> 32) as u32,
        reserved: 0,
    };
    serial_write(b"step: idt int80 set ok\r\n");
    // Registra handlers diretos de exceção (Page Fault e General Protection)
    // para evitar reinício silencioso (triple fault) quando o userspace falha.
    idt[0x0e] = IdtEntry {
        attributes: 0xee,
        ..IdtEntry::interrupt_gate(dreamcore_page_fault_handler as *const () as usize as u64, selector)
    };
    idt[0x0d] = IdtEntry {
        attributes: 0xee,
        ..IdtEntry::interrupt_gate(dreamcore_gp_handler as *const () as usize as u64, selector)
    };
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

    // IA32_STAR = segmento CS (bits 47-32, usado pelo SYSCALL para entrar no
    // kernel com RPL=0) e SS (bits 31-16). Para SYSRET, CS = STAR[47:32]+3 e
    // SS = STAR[15:0]+3, mas usamos iretq (não sysretq) para o retorno, então
    // apenas SYSCALL consome estes campos. STAR[47:32] deve ser o seletor de
    // código do kernel (0x08) para que SYSCALL entre em ring 0 corretamente.
    let star: u64 = ((0x00u64) << 48) | ((0x08u64) << 32) | ((0x10u64) << 16) | (0x00u64);
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
    let star_read = unsafe { rdmsr(0xC0000081) };
    crate::serial_write(b"  STAR=0x");
    crate::serial_write_hex(star_read);
    crate::serial_write(b"\r\n");

    // IA32_FMASK = flags removidas ao entrar em ring 0 (0 = não limpa nada, seguro).
    // Pulado: wrmsr(0xC0000084) causa reset neste QEMU; FMASK=0 é o valor padrão.

    serial_write(b"syscall MSRs configured\r\n");
    let efer = unsafe { rdmsr(0xC0000080) };
    crate::serial_write(b"  EFER=0x");
    crate::serial_write_hex(efer);
    crate::serial_write(b" SCE=");
    crate::serial_write(if efer & 1 != 0 { b"1" } else { b"0" });
    crate::serial_write(b"\r\n");
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
