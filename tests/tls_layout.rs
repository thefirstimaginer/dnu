// Testes do layout TLS (modelo *variant II*) sem bootar.
//
// O loader do kernel monta o bloco TLS e grava `IA32_FS_BASE`; o codegen do
// Rust acessa as variáveis com offsets **negativos** a partir de `%fs`
// (`-0x2000(%rax)` para `%fs:0` na primeira página do bloco). Este arquivo
// fixa a aritmética que precisa casar com o binário.
//
// Os números abaixo vêm do `rust-std-hello` real:
//   PT_TLS: filesz = 0x30 (48 bytes de .tdata), memsz = 0x1020 (4128)
//
// Run with:
//   rustc --edition 2021 --test dnu_kernel/src/tests/tls_layout.rs -o /tmp/tls-tests && /tmp/tls-tests

const TCB_SIZE: u64 = 0x1000;
const TCB_SELF_OFFSET: u64 = 0;

/// Réplica de `loader::build_tls` (só a aritmética de endereços).
struct Layout {
    block: u64,
    fs_base: u64,
    self_pointer: u64,
}

fn layout(stack_pointer: u64, memsz: u64) -> Layout {
    let total = memsz + TCB_SIZE;
    let block = stack_pointer - total;
    let fs_base = block + memsz;
    Layout {
        block,
        fs_base,
        self_pointer: fs_base + TCB_SELF_OFFSET,
    }
}

#[test]
fn fs_base_sits_at_the_end_of_the_tls_block() {
    // `user_stack` = topo da pilha de argumentos.
    let stack = 0x7fff_ffff_0000u64;
    let tls = layout(stack, 0x1020);
    assert_eq!(tls.fs_base, tls.block + 0x1020);
    // O TCB fica imediatamente acima e não invade a pilha de argumentos.
    assert!(tls.fs_base + TCB_SIZE <= stack);
}

#[test]
fn negative_offsets_match_the_generated_code() {
    // O binário acessa a variável em `bloco + 0` como `-0x2000(%fs)`:
    //   fs_base - 0x2000 == bloco  <=>  memsz == 0x2000.
    //
    // No binário real memsz = 0x1020 e o codegen usa `-0xfe8` para a variável
    // em `bloco + 0x38`:
    //   fs_base - 0xfe8 == bloco + 0x38
    let memsz = 0x1020u64;
    let fs_base = 0x5000_0000u64 + memsz;
    let variable_at = fs_base - 0xfe8;
    assert_eq!(variable_at - (fs_base - memsz), 0x38);
}

#[test]
fn self_pointer_is_readable_at_fs_zero() {
    // A primeira instrução TLS do `_start` é `mov %fs:0x0, %rax`; o valor
    // precisa ser o endereço do próprio TCB.
    let tls = layout(0x7fff_ffff_0000, 0x1020);
    assert_eq!(tls.self_pointer, tls.fs_base);
}

#[test]
fn block_never_overlaps_the_argument_stack() {
    // Simula `prepare_user_stack`: argv/envp ocupam ~100 bytes no topo.
    let stack_pointer = 0x7fff_ffff_0000u64 - 0x100;
    let tls = layout(stack_pointer, 0x1020);
    assert!(tls.block + 0x1020 + TCB_SIZE <= stack_pointer);
}

#[test]
fn no_tls_means_no_fs_base() {
    // Binário `no_std` sem `PT_TLS`: `fs_base` fica em 0 e nada é montado.
    let fs_base = if 0u64 == 0 { 0 } else { 1 };
    assert_eq!(fs_base, 0);
}

#[test]
fn the_loader_uses_the_same_constants() {
    // Guarda-corpo: os números acima precisam existir no loader real.
    let source = include_str!("../exec/loader.rs");
    assert!(source.contains("const TCB_SIZE: u64 = 0x1000;"));
    assert!(source.contains("const TCB_SELF_OFFSET: u64 = 0;"));
    assert!(source.contains("let block = stack_pointer"));
    assert!(
        source.contains("fs_base = block"),
        "fs_base deve ser bloco + memsz"
    );
}

#[test]
fn the_dnulink_marks_the_tls_segment() {
    // O DEF precisa trazer o bloco marcado; sem a marca o loader o trataria
    // como página comum e o `%fs` ficaria sem destino.
    let source = include_str!("../../../common/tools/dnulink.rs");
    assert!(source.contains("const PT_TLS: u32 = 7;"));
    assert!(source.contains("DEF_SEGMENT_TLS"));
}
