//! Carregamento de imagens executáveis **DEF** (Dreamcore Executable Format).
//!
//! O DEF é o único formato de userspace aceito: o kernel identifica a imagem
//! pelo magic `DNUF` e rejeita qualquer outra coisa (inclusive ELF) com
//! `LoadError::InvalidDef`.

use core::mem::size_of;
use core::ptr;

use crate::abi::trap::TrapFrame;
use crate::exec::def::{self, Def};
use crate::fpu;
use crate::gdt;
use crate::paging::{AddressSpace, MapError};
use crate::exec::process::{
    FpuState, Priority, Process, KERNEL_STACK_SIZE, KERNEL_STACKS, MAX_EXEC_ARGS, MAX_EXEC_ENV,
    PAGE_SIZE, USER_HEAP_START, USER_STACK_SIZE, USER_STACK_TOP,
};

use super::space::copy_to_space_result;

/// Falhas possíveis ao montar um processo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// O arquivo não é um DEF válido (magic ausente ou cabeçalho incoerente).
    InvalidDef(def::Error),
    OutOfMemory,
    InvalidMapping,
    /// `process::init` recebeu uma lista vazia de programas.
    EmptyPrograms,
}

/// Menor pilha de usuário aceita quando o binário DEF declara a sua própria.
pub const MIN_USER_STACK_SIZE: u64 = 8 * 4096;
/// Maior pilha de usuário aceita quando o binário DEF declara a sua própria.
pub const MAX_USER_STACK_SIZE: u64 = 4096 * 4096;

/// Um segmento carregável, achatado a partir do DEF.
struct LoadedSegment {
    virtual_address: u64,
    memory_size: u64,
    file_offset: usize,
    file_size: usize,
}

/// Um segmento de TLS lido do DEF (`reserved & DEF_SEGMENT_TLS`).
///
/// `image_offset` é o endereço **na imagem TLS**, não um VA: 0 é o início do
/// bloco e `image_offset + sz` o fim dos dados inicializados. O loader copia
/// `file_size` bytes do arquivo para `bloco + image_offset` e zera o resto.
struct TlsSegment {
    image_offset: u64,
    file_offset: usize,
    file_size: usize,
    memory_size: u64,
    /// `p_align` do `PT_TLS` (mínimo 16 — o codegen do Rust alinha o bloco).
    alignment: u64,
}

/// Layout do bloco TLS (modelo *variant II*, o mesmo da musl no x86_64).
///
/// O codegen do Rust no x86_64 acessa variáveis TLS com offsets **negativos**
/// a partir de `%fs` (confirmado na desmontagem do exemplo com `std`:
/// `%fs:0x0` seguido de `-0xfe8(%rax)`). O layout, idêntico ao da musl:
///
/// ```text
///   fs_base + 0x00 ─► [ tcb->self ]  (o código lê %fs:0x0)
///   fs_base + 0x08 ─► [ dtv ]
///   fs_base ────────► fim do bloco TLS == bloco + memsz
///   fs_base - memsz ─► início do bloco (offset 0 da imagem TLS)
/// ```
///
/// Ou seja `fs_base = bloco + memsz`: um `memcpy` da imagem para
/// `fs_base - memsz` reproduz exatamente os offsets que o código espera.
/// (Conferido com o binário: a variável em `bloco + 0x38` é acessada como
/// `-0xfe8(%fs_base)` quando `memsz = 0x1020`.)
///
/// O TCB precisa ter espaço para o `struct pthread` da musl (0x900) porque
/// `pthread_self()` devolve `fs_base` diretamente; 4 KiB deixa folga para o
/// restante do bloco de controle que a musl usa ao criar threads.
const TCB_SIZE: u64 = 0x1000;

/// Deslocamento do self-pointer dentro do TCB (`%fs:0x0`).
const TCB_SELF_OFFSET: u64 = 0;

/// Uma imagem executável já validada (sempre DEF).
type Image<'a> = Def<'a>;

/// Pilha inicial pedida pela imagem, limitada à faixa aceita.
///
/// O DEF declara explicitamente o tamanho da pilha. Valores absurdos são
/// grampeados para que um binário malformado não esgote a memória.
fn stack_size(image: &Image<'_>) -> u64 {
    match image.stack_size {
        0 => USER_STACK_SIZE,
        requested => requested.clamp(MIN_USER_STACK_SIZE, MAX_USER_STACK_SIZE),
    }
}

/// Procura o segmento marcado como TLS (`reserved & DEF_SEGMENT_TLS`).
///
/// Devolve `None` para binários sem TLS (`no_std` simples), que continuam
/// funcionando com `fs_base = 0` — só não podem usar `thread_local!`.
fn find_tls_segment(image: &Image<'_>) -> Result<Option<TlsSegment>, LoadError> {
    for index in 0..image.segment_count() {
        let Some(segment) = image.segment(index).map_err(LoadError::InvalidDef)? else {
            continue;
        };
        if segment.is_tls {
            return Ok(Some(TlsSegment {
                image_offset: segment.virtual_address,
                file_offset: segment.file_offset,
                file_size: segment.file_size,
                memory_size: segment.memory_size,
                alignment: 16,
            }));
        }
    }
    Ok(None)
}

fn loaded_segment(image: &Image<'_>, index: usize) -> Result<Option<LoadedSegment>, LoadError> {
    Ok(image
        .segment(index)
        .map_err(LoadError::InvalidDef)?
        .map(|segment| LoadedSegment {
            virtual_address: segment.virtual_address,
            memory_size: segment.memory_size,
            file_offset: segment.file_offset,
            file_size: segment.file_size,
        }))
}


/// Onde escrever o `TrapFrame` inicial do processo novo.
pub enum FrameLocation {
    /// Topo da pilha de kernel do slot (ninguém mais a usa).
    TopOfSlot(usize),
    /// Endereço explícito (`execve` usa a base da própria pilha).
    At(*mut TrapFrame),
}

/// Endereço na base da pilha de kernel de `slot` onde `execve` monta o frame
/// do novo programa sem sobrepor o frame da syscall em andamento.
pub fn execve_frame_scratch(slot: usize) -> Option<*mut TrapFrame> {
    let stacks = unsafe { &mut *KERNEL_STACKS.0.get() };
    let base = stacks.0[slot].as_mut_ptr() as usize;
    let offset = 2 * size_of::<TrapFrame>();
    if offset >= KERNEL_STACK_SIZE {
        return None;
    }
    Some((base + offset) as *mut TrapFrame)
}

/// Monta um `Process` a partir de uma imagem, sem registrá-lo no escalonador.
#[allow(clippy::too_many_arguments)]
pub fn load(
    image: &[u8],
    pid: usize,
    arguments: &[&[u8]],
    environment: &[&[u8]],
    allows_disk_install: bool,
    name: &str,
    location: FrameLocation,
) -> Result<Process, LoadError> {
    let image = Def::parse(image).map_err(LoadError::InvalidDef)?;
    let address_space = AddressSpace::new_user().ok_or(LoadError::OutOfMemory)?;
    map_segments(&image, &address_space)?;

    let stack_size = stack_size(&image);
    let stack_start = USER_STACK_TOP
        .checked_sub(stack_size)
        .ok_or(LoadError::InvalidMapping)?;
    let mut page = stack_start;
    while page < USER_STACK_TOP {
        address_space
            .map_user_page(page)
            .map_err(|_| LoadError::OutOfMemory)?;
        page += PAGE_SIZE;
    }
    let tls_segment = find_tls_segment(&image)?;
    let (user_stack, user_argv, user_envp) =
        prepare_user_stack(&address_space, arguments, environment)?;

    // O bloco TLS fica **abaixo** do `rsp` inicial (que já aponta para a pilha
    // de argumentos). Colocá-lo em `USER_STACK_TOP` sobreporia `argv`/`envp`.
    // Layout final do topo da pilha:
    //
    //   USER_STACK_TOP ─► strings de argv/envp
    //        rsp ─────► ponteiros de argv, envp, NULLs
    //        ...  ─────► [ bloco TLS ][ TCB ]   (fs_base entre eles)
    //
    // O espaço é virtual e a pilha cresce para baixo, então desperdiçar
    // `memsz + TCB_SIZE` de *gap* não custa RAM — só espaço de endereço.
    let fs_base = match &tls_segment {
        Some(tls) => build_tls(&image, &address_space, tls, user_stack)?,
        None => 0,
    };

    let entry = image.entry;
    if address_space.translate(entry).is_none() {
        return Err(LoadError::InvalidMapping);
    }

    let frame = match location {
        FrameLocation::At(pointer) => pointer,
        FrameLocation::TopOfSlot(slot) => {
            let stacks = unsafe { &mut *KERNEL_STACKS.0.get() };
            let top = stacks.0[slot].as_mut_ptr() as u64 + KERNEL_STACK_SIZE as u64;
            (top as usize - size_of::<TrapFrame>()) as *mut TrapFrame
        }
    };
    unsafe {
        frame.write(initial_frame(entry, user_stack, user_argv, user_envp, arguments, environment));
    }

    let mut process_name = [0; imagineos_abi::PROCESS_NAME_SIZE];
    let copied_name_length = name.len().min(process_name.len());
    process_name[..copied_name_length].copy_from_slice(&name.as_bytes()[..copied_name_length]);
    let mut fpu_state = FpuState::EMPTY;
    let _ = FpuState::_ALIGN_CHECK;
    unsafe {
        fpu::reset(fpu_state.as_mut_ptr());
    }
    Ok(Process {
        pid,
        tgid: pid,
        active: true,
        stopped: false,
        parent: 0,
        zombie: false,
        exit_status: 0,
        waiting_for: 0,
        quantum: 0,
        // Imagens carregadas pelo kernel (PID 1) nascem como supervisor; o
        // `process::init` ajusta o nível conforme o programa. Filhos criados em
        // userspace herdam a prioridade do pai em `fork`/`spawn`.
        priority: Priority::Supervisor,
        slice_used: 0,
        allows_disk_install,
        uid: 0,
        gid: 0,
        is_admin: true,
        username: {
            let mut username = [0; imagineos_abi::ACCOUNT_NAME_SIZE];
            username[..4].copy_from_slice(b"root");
            username
        },
        username_length: 4,
        frame,
        address_space: Some(address_space),
        heap_end: USER_HEAP_START,
        name: process_name,
        pending_signals: 0,
        signal_actions: [imagineos_abi::SIGNAL_DEFAULT; 32],
        signal_restorer: 0,
        signal_frame: None,
        fpu_state,
        tls_size: tls_segment.as_ref().map_or(0, |tls| tls.memory_size),
        tls_alignment: tls_segment.as_ref().map_or(16, |tls| tls.alignment),
        // Bloco TLS montado acima (0 quando o DEF não tem `PT_TLS`).
        fs_base,
        futex_address: 0,
        futex_bitset: imagineos_abi::FUTEX_BITSET_MATCH_ANY,
        futex_sequence: 0,
        wake_deadline: 0,
        mappings: [crate::exec::process::Mapping::EMPTY; crate::exec::process::MAX_MAPPINGS],
        mapping_count: 0,
    })
}

fn map_segments(image: &Image<'_>, address_space: &AddressSpace) -> Result<(), LoadError> {
    for index in 0..image.segment_count() {
        let Some(segment) = loaded_segment(image, index)? else {
            continue;
        };
        let segment_end = segment
            .virtual_address
            .checked_add(segment.memory_size)
            .ok_or(LoadError::InvalidMapping)?;
        let first_page = segment.virtual_address & !(PAGE_SIZE - 1);
        let last_page = segment_end
            .checked_sub(1)
            .ok_or(LoadError::InvalidMapping)?
            & !(PAGE_SIZE - 1);
        let mut page = first_page;
        while page <= last_page {
            match address_space.map_user_page(page) {
                Ok(_) | Err(MapError::AlreadyMapped) => {}
                Err(_) => return Err(LoadError::OutOfMemory),
            }
            page += PAGE_SIZE;
        }
        zero_user_range(address_space, segment.virtual_address, segment.memory_size)?;
        let file_bytes = image
            .file_bytes(segment_file(&segment))
            .ok_or(LoadError::InvalidMapping)?;
        copy_to_space_result(address_space, segment.virtual_address, file_bytes)?;
    }
    Ok(())
}

/// Reconstrói o `def::Segment` que `Def::file_bytes` espera.
fn segment_file(segment: &LoadedSegment) -> def::Segment {
    def::Segment {
        virtual_address: segment.virtual_address,
        memory_size: segment.memory_size,
        file_offset: segment.file_offset,
        file_size: segment.file_size,
        permissions: 0,
        is_tls: false,
    }
}

/// Monta o bloco TLS do processo no topo da pilha do usuário.
///
/// Devolve o valor para `IA32_FS_BASE` ou `0` quando o binário não tem TLS.
///
/// O bloco fica **abaixo** do `stack_pointer` (o `rsp` inicial, que já aponta
/// para os ponteiros de `argv`/`envp`), ocupando `memsz + TCB_SIZE` bytes.
/// Como o layout é *variant II*, `fs_base = topo do bloco` e as variáveis TLS
/// ficam em offsets negativos — exatamente o que o codegen do Rust e a musl
/// esperam.
///
/// `tls` é o segmento `PT_TLS` do DEF: `file_size` bytes inicializados a partir
/// de `image_offset` na imagem, e o resto zerado (`.tbss`).
fn build_tls(
    image: &Image<'_>,
    address_space: &AddressSpace,
    tls: &TlsSegment,
    stack_pointer: u64,
) -> Result<u64, LoadError> {
    let memsz = tls.memory_size;
    if memsz == 0 {
        return Ok(0);
    }
    // `[ bloco TLS (memsz) ][ TCB (TCB_SIZE) ]`; `fs_base` cai entre os dois.
    let total = memsz
        .checked_add(TCB_SIZE)
        .ok_or(LoadError::InvalidMapping)?;
    let block = stack_pointer
        .checked_sub(total)
        .ok_or(LoadError::InvalidMapping)?;

    // Mapeia as páginas do bloco (a pilha já ocupava até `stack_pointer`; o
    // bloco é novo e pode cair em páginas ainda não mapeadas).
    let start = block & !(PAGE_SIZE - 1);
    let end = stack_pointer;
    let mut page = start;
    while page < end {
        match address_space.map_user_page(page) {
            Ok(_) | Err(MapError::AlreadyMapped) => {}
            Err(_) => return Err(LoadError::OutOfMemory),
        }
        page += PAGE_SIZE;
    }

    // Zera o bloco todo (cobre `.tbss` e o TCB).
    zero_user_range(address_space, block, total)?;

    // Copia os dados inicializados da imagem TLS.
    if tls.file_size != 0 {
        let bytes = image
            .file_bytes(def::Segment {
                virtual_address: tls.image_offset,
                memory_size: memsz,
                file_offset: tls.file_offset,
                file_size: tls.file_size,
                permissions: 0,
                is_tls: true,
            })
            .ok_or(LoadError::InvalidMapping)?;
        let destination = block
            .checked_add(tls.image_offset)
            .ok_or(LoadError::InvalidMapping)?;
        copy_to_space_result(address_space, destination, bytes)?;
    }

    // `fs_base` aponta para o fim do bloco (variant II).
    let fs_base = block
        .checked_add(memsz)
        .ok_or(LoadError::InvalidMapping)?;

    // `%fs:0x0` é o *self-pointer* do TCB: o codegen do Rust carrega dele e a
    // musl devolve esse endereço em `pthread_self()`. Deve apontar para si.
    let self_pointer = fs_base
        .checked_add(TCB_SELF_OFFSET)
        .ok_or(LoadError::InvalidMapping)?;
    copy_to_space_result(address_space, self_pointer, &self_pointer.to_le_bytes())?;

    Ok(fs_base)
}

fn initial_frame(
    entry: u64,
    user_stack: u64,
    user_argv: u64,
    user_envp: u64,
    arguments: &[&[u8]],
    environment: &[&[u8]],
) -> TrapFrame {
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
        rdx: environment.len() as u64,
        rcx: user_envp,
        rbx: 0,
        rax: 0,
        rip: entry,
        cs: gdt::USER_CODE_SELECTOR,
        // IF = 1 para o `iretq` ligar as interrupções exatamente ao entrar em
        // ring 3, já com um frame de usuário válido na pilha. Este é o único
        // ponto seguro: o caminho do kernel que monta o frame fica sem
        // interrupções.
        rflags: 0x202,
        rsp: user_stack,
        ss: gdt::USER_DATA_SELECTOR,
        rdi: arguments.len() as u64,
        rsi: user_argv,
    }
}

/// Monta a pilha inicial no padrão SysV: strings, `envp[]`, `argv[]`.
fn prepare_user_stack(
    space: &AddressSpace,
    arguments: &[&[u8]],
    environment: &[&[u8]],
) -> Result<(u64, u64, u64), LoadError> {
    if arguments.len() > MAX_EXEC_ARGS || environment.len() > MAX_EXEC_ENV {
        return Err(LoadError::InvalidMapping);
    }
    let mut stack_pointer = USER_STACK_TOP;
    let mut argument_pointers = [0u64; MAX_EXEC_ARGS];
    let mut environment_pointers = [0u64; MAX_EXEC_ENV];
    for (index, argument) in arguments.iter().enumerate() {
        stack_pointer = stack_pointer
            .checked_sub(argument.len() as u64 + 1)
            .ok_or(LoadError::InvalidMapping)?;
        copy_to_space_result(space, stack_pointer, argument)?;
        copy_to_space_result(space, stack_pointer + argument.len() as u64, &[0])?;
        argument_pointers[index] = stack_pointer;
    }
    for (index, entry) in environment.iter().enumerate() {
        stack_pointer = stack_pointer
            .checked_sub(entry.len() as u64 + 1)
            .ok_or(LoadError::InvalidMapping)?;
        copy_to_space_result(space, stack_pointer, entry)?;
        copy_to_space_result(space, stack_pointer + entry.len() as u64, &[0])?;
        environment_pointers[index] = stack_pointer;
    }

    let environment_vector_size = (environment.len() + 1)
        .checked_mul(size_of::<u64>())
        .ok_or(LoadError::InvalidMapping)? as u64;
    let environment_vector = stack_pointer
        .checked_sub(environment_vector_size)
        .ok_or(LoadError::InvalidMapping)?
        & !0xf;
    for (index, pointer) in environment_pointers
        .iter()
        .take(environment.len())
        .enumerate()
    {
        copy_to_space_result(
            space,
            environment_vector + (index * size_of::<u64>()) as u64,
            &pointer.to_le_bytes(),
        )?;
    }
    copy_to_space_result(
        space,
        environment_vector + (environment.len() * size_of::<u64>()) as u64,
        &0u64.to_le_bytes(),
    )?;

    let argument_vector_size = (arguments.len() + 1)
        .checked_mul(size_of::<u64>())
        .ok_or(LoadError::InvalidMapping)? as u64;
    stack_pointer = environment_vector
        .checked_sub(argument_vector_size)
        .ok_or(LoadError::InvalidMapping)?
        & !0xf;
    for (index, pointer) in argument_pointers.iter().take(arguments.len()).enumerate() {
        copy_to_space_result(
            space,
            stack_pointer + (index * size_of::<u64>()) as u64,
            &pointer.to_le_bytes(),
        )?;
    }
    copy_to_space_result(
        space,
        stack_pointer + (arguments.len() * size_of::<u64>()) as u64,
        &0u64.to_le_bytes(),
    )?;
    let entry_stack = stack_pointer.checked_sub(8).ok_or(LoadError::InvalidMapping)?;
    Ok((entry_stack, stack_pointer, environment_vector))
}

fn zero_user_range(space: &AddressSpace, address: u64, size: u64) -> Result<(), LoadError> {
    let mut current = address;
    let end = address.checked_add(size).ok_or(LoadError::InvalidMapping)?;
    while current < end {
        let destination = space.translate(current).ok_or(LoadError::InvalidMapping)?;
        let available = (PAGE_SIZE - (current & (PAGE_SIZE - 1))).min(end - current);
        unsafe {
            ptr::write_bytes(destination, 0, available as usize);
        }
        current += available;
    }
    Ok(())
}
