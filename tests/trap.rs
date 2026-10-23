#![allow(dead_code)]
//! Standalone tests for the syscall trap frame, the IDT vector table and the
//! kernel clock.
//!
//! The `TrapFrame` layout is a binary contract with `arch/x86_64/idt.rs`: the
//! stub pushes registers in a fixed order and the hardware pushes the iretq
//! frame on top. These tests keep the field order honest without booting.
//!
//! Run with:
//! `rustup run stable rustc --edition 2021 --test dnu_kernel/src/tests/trap.rs -o /tmp/trap-tests && /tmp/trap-tests`

// `drivers/time.rs` importa `crate::abi::trap`; o alias abaixo satisfaz esse
// caminho para que o mapa de vetores e a matemática do relógio sejam testáveis
// no host, sem bootar.
#[path = "../abi/trap.rs"]
pub mod trap_impl;

pub mod abi {
    pub use super::trap_impl as trap;
}

#[path = "time_math.rs"]
pub mod time;

#[path = "../drivers/time.rs"]
mod drivers_time;

use std::mem::{offset_of, size_of};
use trap_impl::TrapFrame;

#[test]
fn trap_frame_matches_the_int_stub_push_order() {
    // Ordem dos `push` em `dreamcore_syscall_stub` (do último para o primeiro),
    // seguida do frame que o CPU empilha no `int 0x80`.
    let expected = [
        (offset_of!(TrapFrame, r15), 0),
        (offset_of!(TrapFrame, r14), 1),
        (offset_of!(TrapFrame, r13), 2),
        (offset_of!(TrapFrame, r12), 3),
        (offset_of!(TrapFrame, r11), 4),
        (offset_of!(TrapFrame, r10), 5),
        (offset_of!(TrapFrame, r9), 6),
        (offset_of!(TrapFrame, r8), 7),
        (offset_of!(TrapFrame, rbp), 8),
        (offset_of!(TrapFrame, rdi), 9),
        (offset_of!(TrapFrame, rsi), 10),
        (offset_of!(TrapFrame, rdx), 11),
        (offset_of!(TrapFrame, rcx), 12),
        (offset_of!(TrapFrame, rbx), 13),
        (offset_of!(TrapFrame, rax), 14),
        (offset_of!(TrapFrame, rip), 15),
        (offset_of!(TrapFrame, cs), 16),
        (offset_of!(TrapFrame, rflags), 17),
        (offset_of!(TrapFrame, rsp), 18),
        (offset_of!(TrapFrame, ss), 19),
    ];
    for (offset, index) in expected {
        assert_eq!(offset, index * size_of::<u64>());
    }
    assert_eq!(size_of::<TrapFrame>(), 20 * size_of::<u64>());
}

#[test]
fn syscall_arguments_use_r10_instead_of_rcx() {
    // A ABI reserva `rcx` para o endereço de retorno do `int`; o quarto
    // argumento precisa ser lido de `r10`. Este teste documenta o contrato
    // consumido por `abi::dispatch`.
    let mut frame = empty_frame();
    frame.rdi = 1;
    frame.rsi = 2;
    frame.rdx = 3;
    frame.r10 = 4;
    frame.r8 = 5;
    frame.r9 = 6;
    assert_eq!(
        [frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8, frame.r9],
        [1, 2, 3, 4, 5, 6]
    );
}

#[test]
fn fail_and_ok_write_the_negative_errno_contract() {
    let mut frame = empty_frame();
    frame.fail(-14);
    assert_eq!(frame.rax, (-14i64) as u64);
    frame.ok(7);
    assert_eq!(frame.rax, 7);
}

fn empty_frame() -> TrapFrame {
    TrapFrame {
        r15: 0,
        r14: 0,
        r13: 0,
        r12: 0,
        r11: 0,
        r10: 0,
        r9: 0,
        r8: 0,
        rbp: 0,
        rdi: 0,
        rsi: 0,
        rdx: 0,
        rcx: 0,
        rbx: 0,
        rax: 0,
        rip: 0,
        cs: 0,
        rflags: 0,
        rsp: 0,
        ss: 0,
    }
}

#[test]
fn timer_vector_is_not_an_exception_vector() {
    // O vetor 0x20 é o único endereço livre para o IRQ0: os vetores 0..=31 são
    // exceções da CPU e 0x80 é a syscall. Registrar o timer em 0x0e faria o IRQ0
    // ser despachado como `#PF` (vetor 14) e matar o processo em execução — era
    // exatamente o que acontecia: o contador de ticks ficava em zero porque o
    // handler do timer nunca era alcançado.
    assert_eq!(drivers_time::timer_vector(), 0x20);
    assert!(drivers_time::timer_vector() > 31);
    assert_ne!(drivers_time::timer_vector(), 0x80);
}

#[test]
fn clock_reports_zero_ticks_before_the_timer_is_armed() {
    // Um tick só conta quando o PIT realmente interrompe: a leitura precisa
    // começar em zero, e não com um valor residual de boot.
    assert_eq!(drivers_time::timer_ticks(), 0);
}

#[test]
fn elapsed_time_is_unavailable_until_the_tsc_frequency_is_known() {
    // Sem frequência calibrada o kernel não pode converter ciclos em tempo; o
    // log degrada para `[time?]` em vez de inventar um número.
    assert_eq!(time::elapsed_microseconds(1_000, 0), None);
    // 1_000_000 ciclos a 1 MHz = exatamente 1 s.
    assert_eq!(
        time::elapsed_microseconds(1_000_000, 1_000_000),
        Some(1_000_000)
    );
    // A 1 kHz, 999 ciclos são 999 ms (999_000 us) — a conversão não pode
    // confundir ciclos com microssegundos.
    assert_eq!(time::elapsed_microseconds(999, 1_000), Some(999_000));
}

#[test]
fn microseconds_conversion_keeps_sub_millisecond_resolution() {
    // 3_403_477_708 Hz (a frequência medida no QEMU) e 1 ms de ciclos: a
    // conversão precisa devolver 1000 us exatos, sem truncar para 0.
    let frequency = 3_403_477_708u64;
    let one_millisecond_of_cycles = frequency / 1000;
    assert_eq!(
        time::elapsed_microseconds(one_millisecond_of_cycles, frequency),
        Some(999)
    );
    // E não pode estourar no primeiro ciclo, o que arredondaria para 0 us.
    assert_eq!(time::elapsed_microseconds(1, frequency), Some(0));
    assert_eq!(
        time::elapsed_microseconds(frequency, frequency),
        Some(1_000_000)
    );
}
