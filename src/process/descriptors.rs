//! Tabela de descritores de arquivo por processo.
//!
//! Cada slot de processo tem sua própria tabela (0=stdin, 1=stdout,
//! 2=stderr). Pontas de pipe são contadas no `crate::ipc`: uma referência é
//! adquirida quando o descritor é instalado/duplicado/herdado e liberada em
//! `close`, na troca de tabela e quando o processo termina. Sem esse balanço,
//! um leitor jamais veria EOF em `ls | cat`.

use core::cell::UnsafeCell;

const MAX_PROCESSES: usize = imagineos_abi::MAX_PROCESSES;

pub const MAX_OPEN_FDS: usize = imagineos_abi::MAX_OPEN_FDS;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DescriptorKind {
    Closed,
    Stdin,
    Stdout,
    Stderr,
    File,
    /// Extremidade de leitura de um pipe (ver `crate::ipc`).
    PipeRead,
    /// Extremidade de escrita de um pipe.
    PipeWrite,
}

#[derive(Clone, Copy)]
pub struct Descriptor {
    pub kind: DescriptorKind,
    pub path: [u8; imagineos_abi::MAX_MUTABLE_PATH],
    pub path_length: usize,
    pub offset: usize,
    pub flags: u64,
    pub input: [u8; 4],
    pub input_length: usize,
    pub input_offset: usize,
    /// Id do pipe quando `kind` é `PipeRead`/`PipeWrite`.
    pub pipe: crate::ipc::PipeId,
}

impl Descriptor {
    pub const CLOSED: Self = Self::new(DescriptorKind::Closed);

    const fn new(kind: DescriptorKind) -> Self {
        Self {
            kind,
            path: [0; imagineos_abi::MAX_MUTABLE_PATH],
            path_length: 0,
            offset: 0,
            flags: 0,
            input: [0; 4],
            input_length: 0,
            input_offset: 0,
            pipe: 0,
        }
    }

    /// Cria um descritor que referencia uma extremidade de pipe.
    pub const fn pipe(kind: DescriptorKind, pipe: crate::ipc::PipeId) -> Self {
        let mut descriptor = Self::new(kind);
        descriptor.pipe = pipe;
        descriptor
    }

    pub fn file(path: &str, flags: u64, offset: usize) -> Self {
        let mut descriptor = Self::new(DescriptorKind::File);
        descriptor.path[..path.len()].copy_from_slice(path.as_bytes());
        descriptor.path_length = path.len();
        descriptor.flags = flags;
        descriptor.offset = offset;
        descriptor
    }

    /// Caminho do arquivo de um descritor `File`.
    pub fn path(&self) -> &str {
        core::str::from_utf8(&self.path[..self.path_length]).unwrap_or("")
    }
}

#[derive(Clone, Copy)]
pub struct Table([Descriptor; MAX_OPEN_FDS]);

impl Table {
    pub const fn new() -> Self {
        let mut descriptors = [Descriptor::CLOSED; MAX_OPEN_FDS];
        descriptors[0] = Descriptor::new(DescriptorKind::Stdin);
        descriptors[1] = Descriptor::new(DescriptorKind::Stdout);
        descriptors[2] = Descriptor::new(DescriptorKind::Stderr);
        Self(descriptors)
    }

    fn allocate(&mut self, descriptor: Descriptor) -> Option<usize> {
        let slot = (3..self.0.len()).find(|&index| self.0[index].kind == DescriptorKind::Closed)?;
        self.0[slot] = descriptor;
        Some(slot)
    }
}

struct SharedTables(UnsafeCell<[Table; MAX_PROCESSES]>);
unsafe impl Sync for SharedTables {}
static TABLES: SharedTables = SharedTables(UnsafeCell::new([Table::new(); MAX_PROCESSES]));

/// Tabela do slot informado (sem lock: o kernel é single-core e só o processo
/// em execução a acessa, salvo durante `fork`/`spawn`, que rodam para o pai).
fn table(slot: usize) -> &'static mut Table {
    unsafe { &mut (*TABLES.0.get())[slot] }
}

pub fn reset_all() {
    unsafe {
        *TABLES.0.get() = [Table::new(); MAX_PROCESSES];
    }
}

pub fn get(slot: usize, fd: usize) -> Option<Descriptor> {
    table(slot)
        .0
        .get(fd)
        .copied()
        .filter(|descriptor| descriptor.kind != DescriptorKind::Closed)
}

pub fn allocate(slot: usize, descriptor: Descriptor) -> Option<usize> {
    table(slot).allocate(descriptor)
}

pub fn update(slot: usize, fd: usize, descriptor: Descriptor) -> bool {
    let Some(slot) = table(slot).0.get_mut(fd) else {
        return false;
    };
    if slot.kind == DescriptorKind::Closed {
        return false;
    }
    *slot = descriptor;
    true
}

pub fn close(slot: usize, fd: usize) -> bool {
    let Some(slot) = table(slot).0.get_mut(fd) else {
        return false;
    };
    let descriptor = *slot;
    if descriptor.kind == DescriptorKind::Closed {
        return false;
    }
    // Libera a referência do pipe antes de descartar o descritor.
    release_pipe_reference(&descriptor);
    *slot = Descriptor::CLOSED;
    true
}

/// Fecha todos os descritores de um slot e libera as referências de pipe.
pub fn release_all(slot: usize) {
    for descriptor in table(slot).0.iter_mut() {
        release_pipe_reference(descriptor);
        *descriptor = Descriptor::CLOSED;
    }
}

/// Instala em `child_slot` as pontas de `parent` indicadas em `io`.
///
/// Um campo `INHERIT` mantém o padrão do filho (stdin/stdout/stderr).
pub fn apply_spawn_io(
    child: &mut Table,
    parent_slot: usize,
    io: &imagineos_abi::SpawnIo,
) {
    let parent = *table(parent_slot);
    let slots = [
        (0usize, io.stdin_fd),
        (1usize, io.stdout_fd),
        (2usize, io.stderr_fd),
    ];
    for (target, source) in slots {
        if source == imagineos_abi::SpawnIo::INHERIT {
            continue;
        }
        let Ok(source) = usize::try_from(source) else {
            continue;
        };
        let Some(descriptor) = parent.0.get(source).copied() else {
            continue;
        };
        if descriptor.kind == DescriptorKind::Closed {
            continue;
        }
        acquire_pipe_reference(&descriptor);
        child.0[target] = descriptor;
    }
}

/// Copia a tabela de `parent` para `child`, contando cada ponta de pipe uma vez
/// a mais (`fork`/`dup`).
pub fn clone_table(parent_slot: usize, child_slot: usize) {
    let parent = *table(parent_slot);
    let child = table(child_slot);
    *child = parent;
    for descriptor in child.0.iter() {
        acquire_pipe_reference(descriptor);
    }
}

/// Prepara a tabela do filho de `spawn`: somente stdio padrão + redireções.
///
/// Deliberadamente **não** herda os fds altos do pai (close-on-exec): herdar
/// fazia cada filho manter referências extras de pipe e `ls | cat` travava.
pub fn prepare_spawn_table(child_slot: usize, parent_slot: usize, io: &imagineos_abi::SpawnIo) {
    let child = table(child_slot);
    *child = Table::new();
    apply_spawn_io(child, parent_slot, io);
}

/// `dup`: novo descritor para o mesmo objeto.
pub fn duplicate(slot: usize, fd: usize) -> Result<usize, i64> {
    let Some(descriptor) = get(slot, fd) else {
        return Err(-9);
    };
    acquire_pipe_reference(&descriptor);
    allocate(slot, descriptor).ok_or_else(|| {
        release_pipe_reference(&descriptor);
        -24 // EMFILE
    })
}

/// `dup2`: faz `target` referenciar o mesmo objeto de `source`.
pub fn duplicate_to(slot: usize, source: usize, target: usize) -> Result<usize, i64> {
    if target >= MAX_OPEN_FDS {
        return Err(-9);
    }
    let Some(descriptor) = get(slot, source) else {
        return Err(-9);
    };
    if source == target {
        return Ok(target);
    }
    // Adquire a referência antes de substituir: se a troca falhar, nada mudou.
    acquire_pipe_reference(&descriptor);
    let cell = &mut table(slot).0[target];
    release_pipe_reference(cell);
    *cell = descriptor;
    Ok(target)
}

fn acquire_pipe_reference(descriptor: &Descriptor) {
    match descriptor.kind {
        DescriptorKind::PipeRead => {
            let _ = crate::ipc::add_reader(descriptor.pipe);
        }
        DescriptorKind::PipeWrite => {
            let _ = crate::ipc::add_writer(descriptor.pipe);
        }
        _ => {}
    }
}

fn release_pipe_reference(descriptor: &Descriptor) {
    match descriptor.kind {
        DescriptorKind::PipeRead => crate::ipc::close_reader(descriptor.pipe),
        DescriptorKind::PipeWrite => crate::ipc::close_writer(descriptor.pipe),
        _ => {}
    }
}
