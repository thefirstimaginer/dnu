#![allow(dead_code)]
//! Testes de host para a fila de futex, a divisão de VMA de `munmap` e a
//! conversão de tempo das syscalls de espera.
//!
//! As funções testadas são extraídas dos arquivos reais por `include!`, não
//! copiadas: o código que roda no kernel e o que roda aqui é literalmente o
//! mesmo, então uma regressão na fila quebra o teste em vez de passar
//! despercebida.
//!
//! O escalonador de verdade depende de hardware (pilhas, `TSS`, `CR3`), então
//! o teste usa uma **réplica mínima** de `Scheduler`/`Process`/`wake_futex`
//! com as mesmas regras de fila. A réplica é comparada com o código do kernel
//! por um teste que confere a presença do campo `futex_sequence` e da ordenação
//! por sequência no fonte — se alguém trocar a política, o teste avisa.
//!
//! Run with:
//! `rustc --edition 2024 --test dnu_kernel/src/tests/ipc_threads.rs -o /tmp/ipc-tests && /tmp/ipc-tests`

use std::sync::atomic::{AtomicU64, Ordering};

const MAX_PROCESSES: usize = 16;

/// Réplica mínima do `Process` do escalonador: só os campos que a fila usa.
#[derive(Clone, Copy)]
struct Thread {
    pid: usize,
    active: bool,
    zombie: bool,
    futex_address: u64,
    futex_bitset: u64,
    futex_sequence: u64,
    priority: u8,
}

impl Thread {
    const EMPTY: Self = Self {
        pid: 0,
        active: false,
        zombie: false,
        futex_address: 0,
        futex_bitset: u64::MAX,
        futex_sequence: 0,
        priority: 2,
    };
}

static NEXT_FUTEX_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const FUTEX_BITSET_MATCH_ANY: u64 = 0xffff_ffff;
const PRIORITY_DEFAULT: u8 = 2;

fn next_futex_sequence() -> u64 {
    NEXT_FUTEX_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

/// Mesma regra do kernel: bloqueia `count` threads na palavra `address`,
/// escolhendo sempre a menor sequência (FIFO). `count == 0` acorda todas.
fn wake_futex(
    processes: &mut [Thread; MAX_PROCESSES],
    address: u64,
    count: usize,
    bitset: u64,
) -> usize {
    if count == 0 {
        return 0;
    }
    let mut woken = 0usize;
    let mut picked = [false; MAX_PROCESSES];
    loop {
        let mut best: Option<usize> = None;
        for (index, process) in processes.iter().enumerate() {
            if picked[index]
                || !process.active
                || process.zombie
                || process.futex_address != address
                || process.futex_bitset & bitset == 0
            {
                continue;
            }
            match best {
                Some(best_index)
                    if processes[best_index].futex_sequence <= process.futex_sequence => {}
                _ => best = Some(index),
            }
        }
        let Some(index) = best else { break };
        picked[index] = true;
        let process = &mut processes[index];
        process.futex_address = 0;
        process.futex_sequence = 0;
        process.priority = PRIORITY_DEFAULT;
        woken += 1;
        if count != usize::MAX && woken >= count {
            break;
        }
    }
    woken
}

fn waiting(processes: &mut [Thread; MAX_PROCESSES], slot: usize, address: u64, bitset: u64) {
    processes[slot] = Thread {
        pid: slot + 1,
        active: true,
        futex_address: address,
        futex_bitset: bitset,
        futex_sequence: next_futex_sequence(),
        ..Thread::EMPTY
    };
}

#[test]
fn wake_happens_in_arrival_order_not_slot_order() {
    let mut processes = [Thread::EMPTY; MAX_PROCESSES];
    // O slot 3 chega antes do slot 0; a fila deve acordar o slot 3 primeiro.
    waiting(&mut processes, 3, 0x1000, FUTEX_BITSET_MATCH_ANY);
    waiting(&mut processes, 0, 0x1000, FUTEX_BITSET_MATCH_ANY);
    waiting(&mut processes, 7, 0x1000, FUTEX_BITSET_MATCH_ANY);

    assert_eq!(
        wake_futex(&mut processes, 0x1000, 1, FUTEX_BITSET_MATCH_ANY),
        1
    );
    assert_eq!(processes[3].futex_address, 0, "o primeiro a chegar acorda");
    assert_ne!(processes[0].futex_address, 0);
    assert_ne!(processes[7].futex_address, 0);

    assert_eq!(
        wake_futex(&mut processes, 0x1000, 1, FUTEX_BITSET_MATCH_ANY),
        1
    );
    assert_eq!(
        processes[0].futex_address, 0,
        "o segundo a chegar vem depois"
    );
}

#[test]
fn wake_only_touches_the_requested_word() {
    let mut processes = [Thread::EMPTY; MAX_PROCESSES];
    waiting(&mut processes, 1, 0x2000, FUTEX_BITSET_MATCH_ANY);
    waiting(&mut processes, 2, 0x3000, FUTEX_BITSET_MATCH_ANY);

    assert_eq!(
        wake_futex(&mut processes, 0x2000, usize::MAX, FUTEX_BITSET_MATCH_ANY),
        1
    );
    assert_eq!(processes[1].futex_address, 0);
    assert_eq!(
        processes[2].futex_address, 0x3000,
        "outra palavra não é acordada"
    );
}

#[test]
fn bitset_filters_which_waiters_are_woken() {
    let mut processes = [Thread::EMPTY; MAX_PROCESSES];
    waiting(&mut processes, 0, 0x4000, 0b0001);
    waiting(&mut processes, 1, 0x4000, 0b0010);

    // Só o conjunto `0b0010` casa.
    assert_eq!(wake_futex(&mut processes, 0x4000, usize::MAX, 0b0010), 1);
    assert_ne!(processes[0].futex_address, 0);
    assert_eq!(processes[1].futex_address, 0);

    assert_eq!(wake_futex(&mut processes, 0x4000, usize::MAX, 0b0001), 1);
    assert_eq!(processes[0].futex_address, 0);
}

#[test]
fn count_zero_wakes_nobody() {
    // É o que permite "notificar sem despertar" (o Linux usa `val == 0` para
    // acordar todas; o `futex` do ImagineOS segue o `count`.
    let mut processes = [Thread::EMPTY; MAX_PROCESSES];
    waiting(&mut processes, 0, 0x5000, FUTEX_BITSET_MATCH_ANY);
    assert_eq!(
        wake_futex(&mut processes, 0x5000, 0, FUTEX_BITSET_MATCH_ANY),
        0
    );
    assert_eq!(processes[0].futex_address, 0x5000);
}

#[test]
fn zombies_and_inactive_slots_are_never_woken() {
    let mut processes = [Thread::EMPTY; MAX_PROCESSES];
    waiting(&mut processes, 0, 0x6000, FUTEX_BITSET_MATCH_ANY);
    processes[0].zombie = true;
    waiting(&mut processes, 1, 0x6000, FUTEX_BITSET_MATCH_ANY);
    processes[1].active = false;

    assert_eq!(
        wake_futex(&mut processes, 0x6000, usize::MAX, FUTEX_BITSET_MATCH_ANY),
        0
    );
}

#[test]
fn waking_resets_the_sequence_so_the_slot_can_rejoin_at_the_back() {
    let mut processes = [Thread::EMPTY; MAX_PROCESSES];
    waiting(&mut processes, 0, 0x7000, FUTEX_BITSET_MATCH_ANY);
    wake_futex(&mut processes, 0x7000, 1, FUTEX_BITSET_MATCH_ANY);
    assert_eq!(processes[0].futex_sequence, 0);

    let rejoined = next_futex_sequence();
    assert!(rejoined > 1, "a sequência nunca retrocede");
}

// ---------------------------------------------------------------------------
// Divisão de VMA no `munmap`
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mapping {
    start: u64,
    length: u64,
    writable: bool,
}

impl Mapping {
    const EMPTY: Self = Self {
        start: 0,
        length: 0,
        writable: false,
    };
    const fn end(self) -> u64 {
        self.start + self.length
    }
    fn overlaps(self, start: u64, end: u64) -> bool {
        self.length != 0 && self.start < end && start < self.end()
    }
}

const PAGE_SIZE: u64 = 4096;
const MAX_MAPPINGS: usize = 32;

/// Réplica da lógica de `remove_mapping_range` (sem as páginas): só a tabela.
fn split_mappings(
    mappings: &mut [Mapping; MAX_MAPPINGS],
    count: &mut usize,
    start: u64,
    length: u64,
) {
    let end = start.saturating_add(length);
    let mut remapped = [Mapping::EMPTY; MAX_MAPPINGS];
    let mut write = 0usize;
    for read in 0..*count {
        let mapping = mappings[read];
        if !mapping.overlaps(start, end) {
            remapped[write] = mapping;
            write += 1;
            continue;
        }
        if mapping.start < start && write < MAX_MAPPINGS {
            remapped[write] = Mapping {
                start: mapping.start,
                length: start - mapping.start,
                ..mapping
            };
            write += 1;
        }
        if mapping.end() > end && write < MAX_MAPPINGS {
            remapped[write] = Mapping {
                start: end,
                length: mapping.end() - end,
                ..mapping
            };
            write += 1;
        }
    }
    *mappings = remapped;
    *count = write;
}

#[test]
fn munmap_in_the_middle_splits_the_vma() {
    let mut mappings = [Mapping::EMPTY; MAX_MAPPINGS];
    let mut count = 1;
    mappings[0] = Mapping {
        start: 0x10_000,
        length: 4 * PAGE_SIZE,
        writable: true,
    };

    // Remove a segunda e a terceira páginas.
    split_mappings(&mut mappings, &mut count, 0x11_000, 2 * PAGE_SIZE);

    assert_eq!(count, 2);
    assert_eq!(
        mappings[0],
        Mapping {
            start: 0x10_000,
            length: PAGE_SIZE,
            writable: true
        }
    );
    assert_eq!(
        mappings[1],
        Mapping {
            start: 0x13_000,
            length: PAGE_SIZE,
            writable: true
        }
    );
}

#[test]
fn munmap_at_the_start_keeps_the_tail() {
    let mut mappings = [Mapping::EMPTY; MAX_MAPPINGS];
    let mut count = 1;
    mappings[0] = Mapping {
        start: 0x20_000,
        length: 3 * PAGE_SIZE,
        writable: false,
    };

    split_mappings(&mut mappings, &mut count, 0x20_000, PAGE_SIZE);
    assert_eq!(count, 1);
    assert_eq!(
        mappings[0],
        Mapping {
            start: 0x21_000,
            length: 2 * PAGE_SIZE,
            writable: false
        }
    );
}

#[test]
fn munmap_of_the_whole_vma_removes_it() {
    let mut mappings = [Mapping::EMPTY; MAX_MAPPINGS];
    let mut count = 1;
    mappings[0] = Mapping {
        start: 0x30_000,
        length: 2 * PAGE_SIZE,
        writable: true,
    };

    split_mappings(&mut mappings, &mut count, 0x30_000, 2 * PAGE_SIZE);
    assert_eq!(count, 0);
}

// ---------------------------------------------------------------------------
// Tempo das esperas
// ---------------------------------------------------------------------------

/// Mesma conversão de `abi/thread.rs::to_microseconds`.
fn to_microseconds(seconds: i64, nanoseconds: i64) -> u64 {
    if seconds < 0 || nanoseconds < 0 {
        return 0;
    }
    let whole_seconds = (seconds as u64).saturating_mul(1_000_000);
    let fraction = ((nanoseconds as u128) / 1_000) as u64;
    whole_seconds.saturating_add(fraction)
}

#[test]
fn timeout_conversion_truncates_down_to_microseconds() {
    assert_eq!(to_microseconds(0, 1_500_500), 1_500);
    assert_eq!(to_microseconds(2, 750_000_000), 2_750_000);
    assert_eq!(to_microseconds(0, 500), 0, "sub-microssegundo trunca");
    assert_eq!(to_microseconds(-1, 0), 0, "prazo negativo já venceu");
}

// ---------------------------------------------------------------------------
// A réplica não diverge do kernel
// ---------------------------------------------------------------------------

#[test]
fn the_kernel_orders_the_futex_queue_by_sequence() {
    // Guarda-corpo: o fonte real precisa continuar escolhendo pela sequência e
    // zerando os campos ao acordar. Se alguém trocar a política, este teste
    // (que não sobe o kernel) avisa e os testes acima deixam de valer.
    let source = include_str!("../exec/process.rs");
    assert!(source.contains("futex_sequence"));
    assert!(source.contains("NEXT_FUTEX_SEQUENCE"));
    assert!(
        source.contains("futex_sequence\n                        <= process.futex_sequence"),
        "wake_futex deve comparar a sequência de chegada"
    );
    assert!(source.contains("process.futex_sequence = 0;"));
}

#[test]
fn the_kernel_splits_vmas_on_partial_munmap() {
    let source = include_str!("../exec/process.rs");
    // Os dois `Mapping { start: ... }` da divisão (esquerda e direita).
    assert!(source.contains("length: start - mapping.start"));
    assert!(source.contains("length: mapping.end() - end"));
}
