use core::arch::asm;

/// Tamanho da área de estado salva por `fxsave`/`fxrstor`.
/// É 512 bytes em x86_64 e deve ser alinhada a 16 bytes.
pub const FXSAVE_SIZE: usize = 512;
pub const FXSAVE_ALIGN: usize = 16;

/// Habilita a FPU e as extensões SSE/SSE2 no processador.
///
/// Em x86_64 o compilador assume SSE2 sempre disponível, mas o hardware só
/// aceita instruções SSE quando `CR4.OSFXSR` está setado (e a FPU está
/// habilitada em `CR0`). Sem esta inicialização, a primeira instrução SSE
/// (por exemplo `pxor`/`movdqa`/`movups`) dispara #UD (vetor 6).
///
/// Deve ser chamada cedo no boot, antes de qualquer código que possa emitir
/// instruções SSE — inclusive código Rust do próprio kernel.
pub fn init() {
    unsafe {
        let mut cr0: u64;
        asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
        cr0 &= !(1 << 2); // EM = 0: usa o hardware, sem emulação da FPU.
        cr0 |= 1 << 1; // MP = 1: monitora o coprocessador.
        cr0 |= 1 << 5; // NE = 1: exceções nativas de FPU (#MF) em vez de IRQ13.
        asm!("mov cr0, {}", in(reg) cr0, options(nomem, nostack, preserves_flags));

        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
        cr4 |= 1 << 9; // OSFXSR: habilita instruções SSE (FXSAVE/FXRSTOR e o conjunto XMM).
        cr4 |= 1 << 10; // OSXMMEXCPT: habilita exceções SIMD (#XM).
        asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack, preserves_flags));

        asm!("fninit", options(nomem, nostack, preserves_flags));
    }
}

/// Salva o estado completo de FPU/SSE na área informada (deve ter
/// `FXSAVE_SIZE` bytes e alinhamento de `FXSAVE_ALIGN`).
///
/// # Safety
/// `area` deve apontar para uma região válida de pelo menos `FXSAVE_SIZE`
/// bytes, alinhada a 16 bytes, gravável pelo kernel.
pub unsafe fn save(area: *mut u8) {
    asm!("fxsave [{}]", in(reg) area, options(nostack, preserves_flags));
}

/// Restaura o estado de FPU/SSE previamente salvo por [`save`].
///
/// # Safety
/// `area` deve conter um estado válido previamente produzido por `fxsave`.
pub unsafe fn restore(area: *const u8) {
    asm!("fxrstor [{}]", in(reg) area, options(nostack, preserves_flags));
}

/// Zera/limpa a área de FXSAVE para um estado inicial previsível.
///
/// Usa `fninit` + `fxsave` sobre a área, garantindo uma imagem válida para
/// processos recém-criados, sem depender do que havia na memória.
///
/// # Safety
/// As mesmas exigências de [`save`].
pub unsafe fn reset(area: *mut u8) {
    asm!("fninit", options(nomem, nostack, preserves_flags));
    save(area);
}
