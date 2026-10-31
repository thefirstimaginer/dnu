// ==== Gerado por tools/abi-bundle.sh — não edite à mão. ====
// Fonte: abi/imagineos_abi/src/*.rs
pub mod imagineos_abi {



pub const ABI_VERSION: u64 = 1; // Não há porque mudar a versão da ABI a cada compilação,
                                // somente se houver transição de versão estável 
                                // (v1.x -> ABI 1, v2.x ABI 2)

// Limites compartilhados entre kernel, libc C (`dreamcore.h`) e runtime Rust.
// O kernel não pode aceitar pedidos maiores que estes, e nenhum deles pode
// divergir de `common/userland/libraries/libc/include/dreamcore.h`.

/// Tamanho máximo de um `write`/`read` em descritor.
pub const MAX_IO_SIZE: usize = 4096;
/// Tamanho máximo de um pedido `SYS_WRITE` (console sem descritor).
pub const MAX_CONSOLE_WRITE: usize = 512;
/// Tamanho máximo de um caminho consultado por `*_file`/`isdir`/`isfile`.
pub const MAX_PATH_QUERY: usize = 128;
/// Tamanho máximo de um caminho mutável (`open`, `mkdir`, `pivot_root`...).
pub const MAX_MUTABLE_PATH: usize = 256;
/// Buffer máximo devolvido por `readdir`/`dmesg`/`read_file`.
pub const MAX_READ_BUFFER: usize = MAX_IO_SIZE;
/// Conteúdo máximo aceito por `write_file`.
pub const MAX_WRITE_FILE_SIZE: usize = MAX_IO_SIZE;

/// Retrato do sistema devolvido por `SYS_SYSTEM_INFO`.
///
/// Todos os campos são valores já calculados pelo kernel: o userspace não tem
/// como consultar o TSC, o alocador de frames ou o escalonador diretamente, e
/// derivar essas informações de syscalls avulsas daria números inconsistentes
/// entre si (uptime medido antes e RAM medida depois, por exemplo).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SystemInfo {
    /// Microssegundos desde o boot, do relógio de TSC.
    pub uptime_microseconds: u64,
    /// Frequência do TSC em Hz (0 quando não calibrada).
    pub tsc_frequency: u64,
    /// RAM utilizável total, em bytes.
    pub memory_total: u64,
    /// RAM ainda livre, em bytes.
    pub memory_free: u64,
    /// Largura e altura do framebuffer ativo (0 se não houver).
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    /// Processos ativos no momento da chamada.
    pub process_count: u32,
    /// Prioridade do processo que fez a chamada (ver `Priority` no kernel).
    pub priority: u32,
    /// PID do processo que fez a chamada.
    pub pid: u64,
    /// Versão da ABI do kernel.
    pub abi_version: u64,
}

/// Flags do `waitpid`: `WNOHANG` — não bloqueia; devolve 0 se nenhum filho
/// terminou. Necessário para supervisores de serviços (Horust) e shells.
pub const WNOHANG: i64 = 1;

/// Superfícies de clientes registráveis por processo; o painel usa 1, um
/// cliente tipo terminal usaria outra.
pub const MAX_SURFACES: usize = 8;
/// Tamanho máximo do buffer de uma superfície, em pixels (320x200 como as
/// janelas internas do compositor).
pub const SURFACE_MAX_WIDTH: u32 = 320;
pub const SURFACE_MAX_HEIGHT: u32 = 200;
pub const SURFACE_MAX_PIXELS: usize =
    (SURFACE_MAX_WIDTH as usize) * (SURFACE_MAX_HEIGHT as usize);

/// Descrição de uma superfície de cliente, preenchida pelo chamador e
/// completada pelo kernel (`address`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceInfo {
    /// Id atribuído pelo kernel (0 se o pedido falhar).
    pub id: u32,
    pub width: u32,
    pub height: u32,
    /// Pixels por linha no buffer mapeado (pode exceder `width`).
    pub pitch: u32,
    /// Endereço virtual do buffer no processo chamador.
    pub address: u64,
}

/// Eventos dirigidos ao dono de uma superfície.
pub const SURFACE_EVENT_KEY: u32 = 1;
pub const SURFACE_EVENT_MOUSE_MOVE: u32 = 2;
pub const SURFACE_EVENT_MOUSE_BUTTON: u32 = 3;
/// O compositor moveu a janela (`x`/`y` são a nova origem na tela).
pub const SURFACE_EVENT_MOVED: u32 = 4;
/// O compositor escondeu a janela (mínima ou fechada pelo WM).
pub const SURFACE_EVENT_HIDDEN: u32 = 5;

/// Snapshot de uma superfície visível, preenchido pelo kernel em
/// `surface_snapshot`. `address` já vem mapeado no espaço do servidor.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceSnapshot {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    /// Endereço virtual do buffer no espaço do servidor de display.
    pub address: u64,
    /// Origem da superfície na tela, definida pelo dono (`surface_move`).
    pub x: i32,
    pub y: i32,
    /// Ordem de empilhamento (maior = à frente).
    pub z: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceEvent {
    /// Superfície alvo.
    pub surface: u32,
    /// Um dos `SURFACE_EVENT_*`.
    pub kind: u32,
    pub x: i32,
    pub y: i32,
    pub button: u32,
    pub pressed: u32,
    /// Caractere (para `SURFACE_EVENT_KEY`).
    pub key: u32,
}

pub const SIGNAL_HUP: u64 = 1;
pub const SIGNAL_INT: u64 = 2;
pub const SIGNAL_KILL: u64 = 9;
pub const SIGNAL_SEGV: u64 = 11;
pub const SIGNAL_TERM: u64 = 15;
pub const SIGNAL_CONT: u64 = 18;
pub const SIGNAL_STOP: u64 = 19;
pub const SIGNAL_DEFAULT: u64 = 0;
pub const SIGNAL_IGNORE: u64 = 1;

pub const PROCESS_RUNNING: u32 = 1;
pub const PROCESS_STOPPED: u32 = 2;
/// Bloqueado em `waitpid` esperando um filho.
pub const PROCESS_WAITING: u32 = 3;
/// Terminou, mas o pai ainda não coletou o status.
pub const PROCESS_ZOMBIE: u32 = 4;
// Large enough for a shell plus a handful of pipeline stages. Each slot costs a
// 64 KiB kernel stack (see `KERNEL_STACKS`), so this is not free memory.
pub const MAX_PROCESSES: usize = 16;
pub const PROCESS_NAME_SIZE: usize = 64;
pub const ACCOUNT_NAME_SIZE: usize = 32;
pub const ACCOUNT_PASSWORD_SIZE: usize = 64;
pub const HOSTNAME_SIZE: usize = 64;

pub const FD_STDIN: u64 = 0;
pub const FD_STDOUT: u64 = 1;
pub const FD_STDERR: u64 = 2;
pub const MAX_OPEN_FDS: usize = 24;

pub const OPEN_READ: u64 = 1 << 0;
pub const OPEN_WRITE: u64 = 1 << 1;
pub const OPEN_CREATE: u64 = 1 << 2;
pub const OPEN_TRUNCATE: u64 = 1 << 3;
pub const OPEN_APPEND: u64 = 1 << 4;
pub const OPEN_EXCLUSIVE: u64 = 1 << 5;

pub const MAX_EXEC_ARGS: usize = 12;
pub const MAX_EXEC_ENV: usize = 12;
pub const MAX_EXEC_ITEM_SIZE: usize = 128;

/// Number of pipes available system-wide (each has a read end and a write end).
pub const MAX_PIPES: usize = 8;
/// Capacity of each pipe ring buffer, in bytes.
pub const PIPE_CAPACITY: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserArg {
    pub address: u64,
    pub length: u64,
}

// ------------------------------------------------------------------------------
// Tipos de suporte à biblioteca padrão (dnu-std)
// ------------------------------------------------------------------------------
/// Descriptor de uma thread criada por `SYS_THREAD_CREATE`.
///
/// A thread nova nasce no mesmo espaço de endereço do processo: memória,
/// descritores e credenciais são compartilhados. O que é próprio de cada thread
/// é a pilha (de kernel e de usuário) e o estado de FPU/SSE.
///
/// `entry` é o endereço da função que a thread começa a executar (recebe
/// `argument` em `rdi`); ao retornar, o kernel chama `SYS_THREAD_EXIT(0)`
/// implicitamente. `stack_top` é o topo da pilha de usuário já alocada pelo
/// chamador (a `std` aloca com `mmap`); `0` significa "o kernel escolhe".
/// `tls` é o valor gravado em `fs_base` para o TLS da thread (0 = nenhum).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThreadCreate {
    /// Função de entrada da thread (`fn(argument)`).
    pub entry: u64,
    /// Valor entregue em `rdi` à função de entrada.
    pub argument: u64,
    /// Topo da pilha de usuário da thread (0 = o kernel escolhe).
    pub stack_top: u64,
    /// Base do TLS da thread, gravada em `fs_base` (0 = não mexe).
    pub tls: u64,
}

/// `futex(&word, op, value, &timeout, ...)`: operações suportadas.
///
/// O subconjunto abaixo cobre `Mutex` (WAIT/WAKE), `Condvar`/`park` (WAKE com
/// contagem) e timeouts (`*_BITSET` com um `Timespec`). A semântica é a mesma do
/// futex do Linux: a espera é **revalidada** sob o bloqueio do escalonador para
/// não perder um `wake` que chegue entre a leitura de `word` e o bloqueio.
pub const FUTEX_WAIT: u64 = 0;
/// `futex(&word, FUTEX_WAKE, count, ...)`: acorda até `count` threads.
pub const FUTEX_WAKE: u64 = 1;
/// Como `FUTEX_WAIT`, mas a espera expira no `Timespec` informado.
pub const FUTEX_WAIT_BITSET: u64 = 9;
/// Como `FUTEX_WAKE`, restrito ao conjunto de bits indicado em `r8`.
pub const FUTEX_WAKE_BITSET: u64 = 10;
/// Máscara que seleciona todas as threads (`FUTEX_BITSET_MATCH_ANY`).
pub const FUTEX_BITSET_MATCH_ANY: u64 = 0xffff_ffff;

/// Relógios de `SYS_CLOCK_GETTIME`.
/// Tempo real (desde a epoch), ajustável — base de `SystemTime`.
pub const CLOCK_REALTIME: u64 = 0;
/// Relógio monótono (nunca retrocede) — base de `Instant` e de `nanosleep`.
pub const CLOCK_MONOTONIC: u64 = 1;

/// Intervalo de tempo em segundos + nanossegundos.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timespec {
    pub seconds: i64,
    pub nanoseconds: i64,
}

/// Flags de `SYS_MMAP`.
/// Mapeamento anônimo (sem arquivo por trás): só páginas zeroadas.
/// A `std` usa isto para o heap e as pilhas de thread.
pub const MAP_ANONYMOUS: u64 = 1 << 0;
/// Mapeamento privado (copy-on-write num `fork` futuro).
pub const MAP_PRIVATE: u64 = 1 << 1;
/// Exige um endereço exato em `address` (falha se já estiver ocupado).
pub const MAP_FIXED: u64 = 1 << 2;

/// Proteções de `SYS_MMAP` (bit a bit).
pub const PROT_READ: u64 = 1 << 0;
pub const PROT_WRITE: u64 = 1 << 1;
pub const PROT_EXEC: u64 = 1 << 2;

/// Tamanho de uma página de usuário, compartilhado por `mmap`/`munmap`.
pub const PAGE_SIZE: u64 = 4096;

/// Descrição do bloco TLS montado pelo kernel (`SYS_TLS_INFO`).
///
/// O kernel monta o bloco a partir do segmento `PT_TLS` do DEF e grava
/// `fs_base` (modelo *variant II*: as variáveis ficam em offsets negativos).
/// `size` é o `p_memsz` do segmento; `0` quando o binário não tem TLS.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TlsInfo {
    pub fs_base: u64,
    pub size: u64,
    pub alignment: u64,
}

/// Maior `length` aceito por uma única `mmap`.
///
/// Sem este limite uma chamada com `length = u64::MAX` derrubaria o kernel ao
/// arredondar a faixa; 64 MiB é muito acima do que a `alloc` da `std` pede de
/// uma vez e continua cabendo com folga no espaço reservado a `mmap`.
pub const MMAP_MAX_LENGTH: u64 = 64 * 1024 * 1024;

/// Base da região de `mmap` anônimo — logo acima da heap de `sbrk`.
///
/// A heap cresce em `USER_HEAP_START` até `USER_HEAP_LIMIT` (64 MiB), então a
/// região anônima começa depois dela. Fica longe das superfícies
/// (`SURFACE_REGION_BASE`), da pilha (`0x7fff_ffff_0000`) e do framebuffer
/// (`0x4000_0000_0000`).
///
/// O kernel distribui as regiões em ordem crescente a partir daqui; o
/// userspace nunca depende de endereços fixos, só do que `mmap` devolve.
pub const MMAP_REGION_BASE: u64 = 0x0000_0002_0000_0000;

/// Teto da região de `mmap` anônimo (exclusivo).
///
/// 256 MiB de espaço virtual: com `MMAP_MAX_LENGTH` = 64 MiB isso comporta
/// várias alocações simultâneas. Como só as páginas tocadas consomem RAM
/// física (o mapeamento é sob demanda), o teto é generoso sem custar memória.
pub const MMAP_REGION_END: u64 = 0x0000_0003_0000_0000;

/// Maior número de threads (além da principal) por processo.
///
/// Cada thread consome um slot do escalonador e uma pilha de kernel de 128 KiB
/// (`KERNEL_STACKS`), então o limite é baixo de propósito. O total de threads
/// do sistema não passa de `MAX_PROCESSES`.
pub const MAX_THREADS_PER_PROCESS: usize = 8;

/// Pilha de usuário que o kernel aloca quando `ThreadCreate.stack_top == 0`.
pub const THREAD_STACK_SIZE: u64 = 128 * 1024;

/// Describes how a child created by `SYS_SPAWN` should see its standard
/// descriptors. Each field is a descriptor number owned by the *caller*: the
/// kernel duplicates it into the child, so `0`, `1`, `2` keep meaning
/// stdin/stdout/stderr. `-1` means "leave it as the default".
///
/// This is how a shell sets up a pipeline (`child 1 | child 2`) without a real
/// `fork`: give the left child `stdout = pipe_write`, the right child
/// `stdin = pipe_read`, and close the unused ends in the parent afterwards.
/// Describes how a child created by `SYS_SPAWN` should see its standard
/// descriptors, and carries the environment for the child.
///
/// The six syscall registers are all used by `SYS_SPAWN`, so the environment
/// (pointer and count) travels inside this structure instead of taking extra
/// registers. Both fields are `u64` so the layout is identical for the kernel
/// and for 64-bit userspace.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnIo {
    pub stdin_fd: i64,
    pub stdout_fd: i64,
    pub stderr_fd: i64,
    /// Ponteiro para um array de [`UserArg`] com o ambiente do filho.
    pub environment: u64,
    /// Quantidade de entradas em `environment`.
    pub environment_count: u64,
}

impl SpawnIo {
    /// Sentinel meaning "keep the default descriptor".
    pub const INHERIT: i64 = -1;
}

/// `Default` inherits every descriptor and passes no environment.
///
/// A caller that only wants to override one descriptor writes just that field:
///
/// ```ignore
/// SpawnIo { stdout_fd: write_end as i64, ..SpawnIo::default() }
/// ```
impl Default for SpawnIo {
    fn default() -> Self {
        Self {
            stdin_fd: Self::INHERIT,
            stdout_fd: Self::INHERIT,
            stderr_fd: Self::INHERIT,
            environment: 0,
            environment_count: 0,
        }
    }
}

/// Descrição do framebuffer para o servidor de display.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FramebufferInfo {
    /// Endereço **virtual** onde o framebuffer aparece no processo chamador,
    /// preenchido por `fb_map`.
    pub address: u64,
    /// Endereço físico da memória de vídeo (informativo).
    pub physical_address: u64,
    pub width: u32,
    pub height: u32,
    /// Bytes por linha (pode ser maior que `width * 4`).
    pub pitch: u32,
    pub red_shift: u8,
    pub green_shift: u8,
    pub blue_shift: u8,
    pub reserved: u8,
}

/// Tipos de evento de entrada entregues ao servidor de display.
pub const INPUT_KEY: u32 = 1;
pub const INPUT_MOUSE_MOVE: u32 = 2;
pub const INPUT_MOUSE_BUTTON: u32 = 3;

/// A base do espaço virtual onde os buffers de superfície são mapeados. Fica
/// bem longe da heap (`USER_HEAP_LIMIT` = 64 MiB sobre `0x1_0000_0000`; a base
/// aqui é +256 MiB) e do framebuffer (`0x4000_0000_0000`). Cada superfície
/// recebe uma fatia alinhada de 1 MiB, então o endereço do cliente nunca
/// colide.
pub const SURFACE_REGION_BASE: u64 = 0x0000_0002_4000_0000;
pub const SURFACE_REGION_STRIDE: u64 = 0x10_0000; // 1 MiB

/// Um evento de entrada em formato fixo (sem ponteiros).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputEvent {
    /// Um dos `INPUT_*`.
    pub kind: u32,
    /// Tecla (para `INPUT_KEY`): o caractere ou um código de tecla especial.
    pub key: u32,
    /// Posição do cursor (para eventos de mouse).
    pub x: i32,
    pub y: i32,
    /// Botão (1 = esquerdo, 2 = direito, 3 = meio).
    pub button: u32,
    /// 1 = pressionado, 0 = liberado.
    pub pressed: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserStat {
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u64,
    /// Um dos `PROCESS_*`.
    pub state: u32,
    pub pending_signals: u32,
    pub name: [u8; PROCESS_NAME_SIZE],
    /// Nível de prioridade (0 = mais baixo, ver `Priority` no kernel).
    pub priority: u32,
    /// 1 quando este é o processo em execução no momento da chamada.
    pub running: u32,
    /// Ticks de CPU que a fatia deste processo concede por vez.
    pub quantum_ticks: u32,
    pub _reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstallConfig {
    pub username_length: u8,
    pub password_length: u8,
    pub hostname_length: u8,
    pub add_user: u8,
    pub administrator: u8,
    pub reserved: [u8; 3],
    pub username: [u8; ACCOUNT_NAME_SIZE],
    pub password: [u8; ACCOUNT_PASSWORD_SIZE],
    pub hostname: [u8; HOSTNAME_SIZE],
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            username_length: 0,
            password_length: 0,
            hostname_length: 0,
            add_user: 0,
            administrator: 0,
            reserved: [0; 3],
            username: [0; ACCOUNT_NAME_SIZE],
            password: [0; ACCOUNT_PASSWORD_SIZE],
            hostname: [0; HOSTNAME_SIZE],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserIdentity {
    pub uid: u32,
    pub gid: u32,
    pub is_admin: u32,
    pub username_length: u32,
    pub username: [u8; ACCOUNT_NAME_SIZE],
    pub hostname_length: u32,
    pub hostname: [u8; HOSTNAME_SIZE],
}

impl Default for UserIdentity {
    fn default() -> Self {
        Self {
            uid: 0,
            gid: 0,
            is_admin: 0,
            username_length: 0,
            username: [0; ACCOUNT_NAME_SIZE],
            hostname_length: 0,
            hostname: [0; HOSTNAME_SIZE],
        }
    }
}

impl Default for ProcessInfo {
    fn default() -> Self {
        Self {
            pid: 0,
            state: 0,
            pending_signals: 0,
            name: [0; PROCESS_NAME_SIZE],
            priority: 0,
            running: 0,
            quantum_ticks: 0,
            _reserved: 0,
        }
    }
}

impl UserArg {
    pub const EMPTY: Self = Self {
        address: 0,
        length: 0,
    };
}
pub mod def {
//! Formato **DEF** (Dreamcore Executable Format) — o contêiner executável
//! nativo do ImagineOS.
//!
//! O DEF é o formato executável nativo do ImagineOS. Ele foi desenhado para
//! ser **zero-copy**: o cabeçalho é uma estrutura de tamanho fixo, alinhada a 8
//! bytes, lida diretamente dos bytes do arquivo (um `#[repr(C)]` que o loader
//! do kernel pode sobrepor na página carregada sem copiar nem realocar nada).
//!
//! Os arquivos DEF **não** usam extensão: são instalados como `/bin/ls`,
//! `/sbin/init` e assim por diante. A extensão `.xapp` pertence ao pacote de
//! aplicação do framework Liminal, que é um artefato separado (ver
//! `documentation/appframework.md`).
//!
//! Comparado a formatos genéricos como o ELF, o DEF:
//!
//! * não tem *section headers* (a visão de linking não importa em tempo de
//!   execução) — só a visão de execução, uma tabela compacta de segmentos;
//! * declara explicitamente o tamanho da pilha, o alinhamento de memória
//!   preferido e se guard pages estão habilitadas;
//! * carrega a versão da ABI do SO no cabeçalho, então o kernel pode recusar
//!   binários compilados para uma ABI antiga com uma mensagem clara em vez de
//!   causar falhas de segmentação misteriosas;
//! * reserva espaço para uma assinatura criptográfica (Ed25519) e para um mapa
//!   de capacidades (quais syscalls/dispositivos o binário pode usar), aplicadas
//!   pelo kernel **antes** da primeira instrução `_start` rodar.
//!
//! Layout do arquivo (tudo little-endian):
//!
//! ```text
//!   offset  tamanho  campo
//!   0       4        magic "DNUF" (0x44 0x4E 0x55 0x46)
//!   4       1        arch (DefArch)
//!   5       1        binary_type (DefType)
//!   6       2        format_version
//!   8       4        abi_version (versão da ABI do SO que o binário espera)
//!   12      4        flags (DefFlags)
//!   16      4        segment_count
//!   20      4        reserved (alinhamento / uso futuro)
//!   24      8        entry_point (endereço virtual)
//!   32      8        stack_size
//!   40      8        image_base (menor endereço virtual mapeado)
//!   48      8        image_size (extensão total do mapa de memória)
//!   56      32       capabilities (bitmap de capacidades concedidas)
//!   88      64       signature (Ed25519; zerado = não assinado)
//!   152     ...      tabela de segmentos (DefSegment[], segment_count entradas)
//!   ...     ...      payloads (código e dados brutos)
//! ```
//!
//! Cada entrada da tabela de segmentos tem [`DEF_SEGMENT_SIZE`] bytes:
//!
//! ```text
//!   offset  tamanho  campo
//!   0       8        file_offset (onde o payload começa no arquivo)
//!   8       8        file_size (bytes a copiar do arquivo)
//!   16      8        memory_size (bytes a reservar na memória; >= file_size)
//!   24      8        virtual_address
//!   32      4        permissions (DefPermissions: R/W/X)
//!   36      4        reserved
//! ```

/// Assinatura do cabeçalho DEF: "DNUF" (Dreamcore Native Universal Format).
pub const DEF_MAGIC: [u8; 4] = *b"DNUF";

/// Versão atual do formato DEF. Independente de [`super::ABI_VERSION`]: o
/// formato do arquivo pode evoluir sem quebrar a ABI das syscalls.
pub const DEF_FORMAT_VERSION: u16 = 1;

/// Tamanho em bytes do cabeçalho DEF fixo.
pub const DEF_HEADER_SIZE: usize = 152;

/// Tamanho em bytes de uma entrada da tabela de segmentos.
pub const DEF_SEGMENT_SIZE: usize = 40;

/// Número máximo de segmentos carregáveis aceito pelo loader.
pub const DEF_MAX_SEGMENTS: usize = 128;

/// Arquitetura-alvo do binário.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DefArch {
    X86_64 = 0x01,
    AArch64 = 0x02,
    RiscV64 = 0x03,
}

impl DefArch {
    /// Arquitetura da máquina que está compilando/rodando este código.
    pub const NATIVE: Self = Self::X86_64;

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::X86_64),
            0x02 => Some(Self::AArch64),
            0x03 => Some(Self::RiscV64),
            _ => None,
        }
    }
}

/// Tipo de binário DEF. A distinção permite que o kernel trate apps, servidores
/// híbridos e drivers de usuário de forma diferente (prioridade, capacidades
/// default, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DefType {
    /// Aplicativo comum em Ring 3.
    App = 0x00,
    /// Servidor híbrido (VFS, rede, display) — roda isolado em Ring 3.
    HybridServer = 0x01,
    /// Driver em espaço de usuário.
    UserDriver = 0x02,
    /// Runtime/interpretador (ex.: engine da NajaScript).
    Runtime = 0x03,
}

impl DefType {
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(Self::App),
            0x01 => Some(Self::HybridServer),
            0x02 => Some(Self::UserDriver),
            0x03 => Some(Self::Runtime),
            _ => None,
        }
    }
}

/// Flags de segurança/assinatura do cabeçalho.
pub struct DefFlags;

impl DefFlags {
    /// O binário traz uma assinatura Ed25519 válida em [`DefHeader::signature`].
    pub const SIGNED: u32 = 1 << 0;
    /// O binário declara capacidades explícitas em [`DefHeader::capabilities`].
    pub const HAS_CAPABILITIES: u32 = 1 << 1;
    /// O loader deve aplicar guard pages no fundo da pilha.
    pub const STACK_GUARD: u32 = 1 << 2;
}

/// Permissões de um segmento (bitmask de página).
pub struct DefPermissions;

impl DefPermissions {
    pub const READ: u32 = 1 << 0;
    pub const WRITE: u32 = 1 << 1;
    pub const EXECUTE: u32 = 1 << 2;

    /// Um segmento de código: executável e somente-leitura (W^X).
    pub const TEXT: u32 = Self::READ | Self::EXECUTE;
    /// Um segmento de dados: leitura/escrita, nunca executável (W^X).
    pub const DATA: u32 = Self::READ | Self::WRITE;
    /// Dados somente-leitura (tabelas de constantes, `.rodata`).
    pub const RODATA: u32 = Self::READ;
}

/// Capacidades declaradas pelo binário. O kernel lê este bitmap e aplica a
/// sandbox antes de entrar em Ring 3. Bit `n` corresponde a
/// [`def_capability_bit`].
pub struct DefCapability;

impl DefCapability {
    /// Acesso ao sistema de arquivos (VFS).
    pub const FS: u64 = 1 << 0;
    /// Acesso à rede (stack TCP/IP).
    pub const NET: u64 = 1 << 1;
    /// Acesso a dispositivos (E/S de portas, MMIO de driver).
    pub const DEVICE: u64 = 1 << 2;
    /// Criar outros processos (`spawn`/`execve`).
    pub const SPAWN: u64 = 1 << 3;
    /// Criar threads.
    pub const THREAD: u64 = 1 << 4;
    /// Servidor gráfico / framebuffer.
    pub const DISPLAY: u64 = 1 << 5;
    /// Gerenciar o ciclo de vida de outros processos (supervisor/init).
    pub const SUPERVISE: u64 = 1 << 6;
    /// Instalar/gerenciar o disco (ex.: instalador).
    pub const INSTALL: u64 = 1 << 7;
}

/// Bit de `DefSegment::reserved` que marca o segmento de **TLS**.
///
/// O DEF não tem um tipo de segmento próprio: o loader carrega tudo como
/// páginas. O bloco thread-local, porém, não é uma página comum — o kernel
/// precisa reconhecê-lo para montar o TCB e gravar `fs_base`. Usar o campo
/// `reserved` (previsto para isso) mantém o formato compatível: binários
/// antigos têm `reserved == 0` e continuam válidos.
pub const DEF_SEGMENT_TLS: u32 = 1 << 0;

/// Um segmento carregável, como fica descrito na tabela do DEF.
///
/// Esta é a visão *em memória* de uma entrada da tabela; o loader do kernel
/// preenche essa estrutura a partir dos bytes crus (ver `dnu/src/exec_def.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefSegment {
    pub file_offset: u64,
    pub file_size: u64,
    pub memory_size: u64,
    pub virtual_address: u64,
    pub permissions: u32,
    pub reserved: u32,
}

impl DefSegment {
    pub const EMPTY: Self = Self {
        file_offset: 0,
        file_size: 0,
        memory_size: 0,
        virtual_address: 0,
        permissions: 0,
        reserved: 0,
    };

    pub const fn is_executable(&self) -> bool {
        self.permissions & DefPermissions::EXECUTE != 0
    }

    pub const fn is_writable(&self) -> bool {
        self.permissions & DefPermissions::WRITE != 0
    }

    /// Este segmento é o bloco `PT_TLS` (`.tdata` + `.tbss`).
    pub const fn is_tls(&self) -> bool {
        self.reserved & DEF_SEGMENT_TLS != 0
    }
}

/// Cabeçalho DEF de tamanho fixo (ver o layout no topo do módulo).
///
/// É `#[repr(C)]` de propósito: o loader zero-copy do kernel sobrepõe este
/// tipo diretamente sobre os bytes do arquivo carregado, sem realocar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DefHeader {
    pub magic: [u8; 4],
    pub arch: u8,
    pub binary_type: u8,
    pub format_version: u16,
    pub abi_version: u32,
    pub flags: u32,
    pub segment_count: u32,
    pub reserved: u32,
    pub entry_point: u64,
    pub stack_size: u64,
    pub image_base: u64,
    pub image_size: u64,
    pub capabilities: [u8; 32],
    pub signature: [u8; 64],
}

impl DefHeader {
    pub const EMPTY: Self = Self {
        magic: DEF_MAGIC,
        arch: DefArch::NATIVE as u8,
        binary_type: DefType::App as u8,
        format_version: DEF_FORMAT_VERSION,
        abi_version: super::ABI_VERSION as u32,
        flags: 0,
        segment_count: 0,
        reserved: 0,
        entry_point: 0,
        stack_size: 0,
        image_base: 0,
        image_size: 0,
        capabilities: [0; 32],
        signature: [0; 64],
    };

    pub const fn magic_ok(&self) -> bool {
        self.magic[0] == DEF_MAGIC[0]
            && self.magic[1] == DEF_MAGIC[1]
            && self.magic[2] == DEF_MAGIC[2]
            && self.magic[3] == DEF_MAGIC[3]
    }

    /// Bitmap de capacidades como um inteiro de 64 bits (bits 0..64).
    pub const fn capability_bits(&self) -> u64 {
        u64::from_le_bytes([
            self.capabilities[0],
            self.capabilities[1],
            self.capabilities[2],
            self.capabilities[3],
            self.capabilities[4],
            self.capabilities[5],
            self.capabilities[6],
            self.capabilities[7],
        ])
    }

    pub const fn has_capability(&self, capability: u64) -> bool {
        self.capability_bits() & capability != 0
    }

    pub const fn is_signed(&self) -> bool {
        self.flags & DefFlags::SIGNED != 0
    }

    pub const fn stack_guard(&self) -> bool {
        self.flags & DefFlags::STACK_GUARD != 0
    }

    /// Offset da tabela de segmentos no arquivo (logo após o cabeçalho fixo).
    pub const fn segment_table_offset(&self) -> u64 {
        DEF_HEADER_SIZE as u64
    }
}

/// Erros de validação do cabeçalho DEF, compartilhados entre o loader do
/// kernel e a ferramenta `dnu-pack` (que valida o que acabou de escrever).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefError {
    /// O arquivo é menor que o cabeçalho DEF.
    Truncated,
    /// Os 4 primeiros bytes não são "DNUF".
    InvalidMagic,
    /// Arquitetura diferente da nativa do SO.
    UnsupportedArch,
    /// Tipo de binário desconhecido.
    UnsupportedType,
    /// Versão de formato DEF não suportada.
    UnsupportedFormatVersion,
    /// Versão da ABI do binário incompatível com a do kernel.
    IncompatibleAbi,
    /// Tabela de segmentos truncada, vazia ou com entradas demais.
    InvalidSegments,
    /// Um segmento aponta para fora do arquivo ou tem tamanhos incoerentes.
    InvalidSegment,
    /// Nenhum segmento contém o `entry_point`.
    InvalidEntry,
}

/// Valida o cabeçalho DEF e a tabela de segmentos contra a imagem inteira.
///
/// É a checagem *estrutural* comum a kernel e ferramenta. Não valida a
/// assinatura criptográfica (isso é política do kernel, em [`DefHeader::signature`]).
///
/// * `image` são os bytes completos do arquivo DEF;
/// * `abi_version` é a ABI do SO que está carregando (o kernel passa
///   [`super::ABI_VERSION`]; a ferramenta passa a mesma para simular o loader);
/// * `native_arch` é a arquitetura do SO.
///
/// Retorna o número de segmentos carregáveis em caso de sucesso.
pub fn validate_image(
    image: &[u8],
    abi_version: u64,
    native_arch: DefArch,
) -> Result<u32, DefError> {
    if image.len() < DEF_HEADER_SIZE {
        return Err(DefError::Truncated);
    }
    let mut magic = [0u8; 4];
    magic.copy_from_slice(&image[..4]);
    if magic != DEF_MAGIC {
        return Err(DefError::InvalidMagic);
    }
    let arch = DefArch::from_u8(image[4]).ok_or(DefError::UnsupportedArch)?;
    if arch != native_arch {
        return Err(DefError::UnsupportedArch);
    }
    if DefType::from_u8(image[5]).is_none() {
        return Err(DefError::UnsupportedType);
    }
    let format_version = u16::from_le_bytes([image[6], image[7]]);
    if format_version != DEF_FORMAT_VERSION {
        return Err(DefError::UnsupportedFormatVersion);
    }
    let binary_abi = u32::from_le_bytes([image[8], image[9], image[10], image[11]]) as u64;
    if binary_abi > abi_version {
        // Um binário que exige uma ABI mais nova do que o kernel oferece não
        // pode rodar: recusamos cedo, com erro claro.
        return Err(DefError::IncompatibleAbi);
    }
    let segment_count = u32::from_le_bytes([image[16], image[17], image[18], image[19]]);
    if segment_count == 0 || segment_count as usize > DEF_MAX_SEGMENTS {
        return Err(DefError::InvalidSegments);
    }
    let table_end = DEF_HEADER_SIZE + segment_count as usize * DEF_SEGMENT_SIZE;
    if image.len() < table_end {
        return Err(DefError::InvalidSegments);
    }
    let entry_point = u64::from_le_bytes([
        image[24], image[25], image[26], image[27], image[28], image[29], image[30], image[31],
    ]);

    // Confere cada entrada da tabela: offsets dentro do arquivo, tamanhos
    // coerentes (memory_size >= file_size) e W^X (nenhum segmento é executável
    // *e* gravável ao mesmo tempo).
    let mut entry_covered = false;
    for index in 0..segment_count as usize {
        let base = DEF_HEADER_SIZE + index * DEF_SEGMENT_SIZE;
        let file_offset = read_u64(image, base);
        let file_size = read_u64(image, base + 8);
        let memory_size = read_u64(image, base + 16);
        let virtual_address = read_u64(image, base + 24);
        let permissions = u32::from_le_bytes([
            image[base + 32],
            image[base + 33],
            image[base + 34],
            image[base + 35],
        ]);
        if memory_size < file_size {
            return Err(DefError::InvalidSegment);
        }
        if file_size != 0 {
            // `file_offset` é um offset *absoluto* na imagem, começando logo após
            // a tabela de segmentos. Não some `base` a ele: `base` já é a posição
            // da entrada dentro da tabela.
            let file_end = file_offset.saturating_add(file_size);
            if file_offset < table_end as u64 || file_end > image.len() as u64 {
                return Err(DefError::InvalidSegment);
            }
        }
        let writable = permissions & DefPermissions::WRITE != 0;
        let executable = permissions & DefPermissions::EXECUTE != 0;
        if writable && executable {
            // Violação de W^X.
            return Err(DefError::InvalidSegment);
        }
        if executable
            && entry_point >= virtual_address
            && entry_point < virtual_address.saturating_add(memory_size)
        {
            entry_covered = true;
        }
    }
    if !entry_covered {
        return Err(DefError::InvalidEntry);
    }
    Ok(segment_count)
}

/// Lê um u64 little-endian a partir de um offset, sem pânico.
fn read_u64(image: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        image[offset],
        image[offset + 1],
        image[offset + 2],
        image[offset + 3],
        image[offset + 4],
        image[offset + 5],
        image[offset + 6],
        image[offset + 7],
    ])
}

// As invariantes de layout do DEF (`size_of`, alinhamento e magic) são
// verificadas no binário `dnu/tools/dnu-pack.rs`, que é um programa de host com
// harness de teste, e na suíte `dnu/tests/`, que roda via `make -C dnu test`.
}
pub mod syscall {
//! Números das syscalls do contrato `int 0x80` do Dreamcore.
//!
//! Formato dos argumentos (64 bits): `rax` = número, `rdi`, `rsi`, `rdx`,
//! `r10`, `r8`, `r9` = argumentos 1..6. O `rcx` **não** carrega argumento: a
//! instrução `int` grava nele o endereço de retorno em ring 3, por isso o
//! quarto parâmetro viaja em `r10`.

pub const SYS_WRITE: u64 = 1;
pub const SYS_READ: u64 = 2;
pub const SYS_YIELD: u64 = 3;
pub const SYS_EXIT: u64 = 4;
pub const SYS_GETPID: u64 = 5;
pub const SYS_CLEAR: u64 = 6;
pub const SYS_EXEC: u64 = 7;
pub const SYS_ISDIR: u64 = 8;
pub const SYS_ISFILE: u64 = 9;
pub const SYS_READ_FILE: u64 = 10;
pub const SYS_READDIR: u64 = 11;
pub const SYS_MKDIR: u64 = 12;
pub const SYS_TOUCH: u64 = 13;
pub const SYS_REMOVE: u64 = 14;
pub const SYS_WRITE_FILE: u64 = 15;
pub const SYS_ABI_VERSION: u64 = 16;
pub const SYS_OPEN: u64 = 17;
pub const SYS_READ_FD: u64 = 18;
pub const SYS_WRITE_FD: u64 = 19;
pub const SYS_CLOSE: u64 = 20;
pub const SYS_DISK_COUNT: u64 = 21;
pub const SYS_DISK_SECTORS: u64 = 22;
pub const SYS_INSTALL_DISK: u64 = 23;
pub const SYS_STAT: u64 = 24;
pub const SYS_DMESG: u64 = 25;
pub const SYS_SHUTDOWN_REQUEST: u64 = 26;
pub const SYS_SHUTDOWN_POLL: u64 = 27;
pub const SYS_POWER_OFF: u64 = 28;
pub const SYS_KILL: u64 = 29;
pub const SYS_SIGACTION: u64 = 30;
pub const SYS_SIGRETURN: u64 = 31;
pub const SYS_PROCESS_LIST: u64 = 32;
pub const SYS_AUTHENTICATE: u64 = 33;
pub const SYS_GETIDENTITY: u64 = 34;
pub const SYS_INSTALL_DISK_CONFIG: u64 = 35;
pub const SYS_SBRK: u64 = 36;
pub const SYS_LSEEK: u64 = 37;
pub const SYS_FSTAT: u64 = 38;
pub const SYS_GETTIMEOFDAY: u64 = 39;
// Primitivas de processo e IPC para shells e pipelines no estilo POSIX.
pub const SYS_DUP: u64 = 40;
pub const SYS_DUP2: u64 = 41;
pub const SYS_PIPE: u64 = 42;
pub const SYS_SPAWN: u64 = 43;
pub const SYS_WAIT: u64 = 44;
/// `fork(2)`: duplica o processo atual. Devolve 0 no filho, o pid no pai.
pub const SYS_FORK: u64 = 45;
/// `waitpid(pid, &status, flags)`: espera um filho e coleta o status.
pub const SYS_WAITPID: u64 = 46;
/// `execve(path, argv, envp)`: substitui a imagem do processo atual.
pub const SYS_EXECVE: u64 = 47;
// Primitivas de display: um servidor de janelas em userspace precisa enxergar
// o framebuffer e receber os eventos de entrada.
/// `fb_info(&FramebufferInfo)`: descreve o framebuffer ativo.
pub const SYS_FB_INFO: u64 = 48;
/// `fb_map(&FramebufferInfo)`: mapeia o framebuffer no processo atual.
pub const SYS_FB_MAP: u64 = 49;
/// `input_poll(&InputEvent)`: retira um evento de entrada (0 se não houver).
pub const SYS_INPUT_POLL: u64 = 50;
/// `console_release()`: o servidor de display assume a tela e o console do
/// kernel deixa de desenhar nela.
pub const SYS_CONSOLE_RELEASE: u64 = 51;
/// `pointer_position()`: devolve `(x << 32) | y` do ponteiro, para o servidor
/// de display desenhar o cursor sem recontar cada movimento.
pub const SYS_POINTER_POSITION: u64 = 52;
// Protocolo de superfícies (janelas cliente/servidor):
// um processo qualquer pode registrar uma superfície de pixels e o servidor
// de display compõe na tela e encaminha eventos. O dono é identificado pelo
// PID, então não há tokens nem permissões — como o X11 antes da autenticação.
/// `surface_create(&SurfaceInfo)`: registra uma superfície no kernel e mapeia
/// o buffer compartilhado no processo atual. `SurfaceInfo.address` volta com o
/// endereço virtual.
pub const SYS_SURFACE_CREATE: u64 = 53;
/// `surface_present(id, flags)`: marca a superfície como visível (1) ou
/// oculta (0) e pede recomposição. Devolve 0 em caso de sucesso.
pub const SYS_SURFACE_PRESENT: u64 = 54;
/// `surface_flush(id)`: publica um frame escrito no buffer; o compositor
/// passa a copiar da região mapeada. Sem esta chamada o cliente desenha
/// "no escuro" (o kernel só copia quando o frame está consistente).
pub const SYS_SURFACE_FLUSH: u64 = 55;
/// `surface_destroy(id)`: remove a superfície, desmapeia e libera o buffer.
pub const SYS_SURFACE_DESTROY: u64 = 56;
/// `surface_poll(&SurfaceEvent)`: retira um evento dirigido a uma superfície
/// deste processo (0 se não houver). O kernel roteia com base no dono.
pub const SYS_SURFACE_POLL: u64 = 57;
/// `surface_snapshot(&[SurfaceSnapshot]) -> count`: lista as superfícies
/// visíveis e mapeia os buffers no processo chamador (o servidor).
pub const SYS_SURFACE_SNAPSHOT: u64 = 58;
/// `surface_send_event(&SurfaceEvent)`: entrega um evento ao dono da
/// superfície indicada (o servidor encaminha teclas e cliques).
pub const SYS_SURFACE_SEND_EVENT: u64 = 59;
/// `pivot_root(new_root, put_old)`: troca a raiz do sistema (initramfs → raiz
/// real no disco). POSIX/Linux: o initramfs chama após montar a nova raiz.
pub const SYS_PIVOT_ROOT: u64 = 60;
/// `surface_move(id, x, y)`: o dono reposiciona a própria superfície na tela.
/// O display server compõe no novo endereço a partir do próximo quadro.
pub const SYS_SURFACE_MOVE: u64 = 61;
/// `system_info(&SystemInfo)`: retrato do sistema (uptime, RAM, resolução,
/// contagem de processos, prioridade do chamador). Usado por `systemfetch` e
/// `top`.
pub const SYS_SYSTEM_INFO: u64 = 62;

// ------------------------------------------------------------------------------
// Base para a biblioteca padrão (dnu-std) do userspace.
//
// Sem estas primitivas não há como construir uma `std` de Rust: `std::thread`
// precisa de `clone`/TLS, `std::sync` precisa de `futex`, `std::time::Instant`
// e `std::thread::sleep` precisam de um relógio monótono e de `nanosleep`, e o
// `alloc` global precisa de `mmap`/`munmap` para crescer e devolver memória.
// ------------------------------------------------------------------------------
/// `thread_create(&ThreadCreate) -> tid`: cria uma thread no espaço de endereço
/// do processo atual. A thread nova compartilha memória, descritores e
/// credenciais com o processo; cada thread tem sua própria pilha de kernel e de
/// usuário. Devolve o `tid` (igual ao `pid` para a thread principal) ou um
/// errno negativo.
///
/// A thread nova começa executando `ThreadCreate.entry`, com `ThreadCreate.argument`
/// em `rdi`. Ao retornar, chama `thread_exit(0)` implicitamente.
pub const SYS_THREAD_CREATE: u64 = 63;
/// `thread_exit(status) -> !`: encerra a thread atual. Se for a última thread do
/// processo, o processo inteiro termina com `status`.
pub const SYS_THREAD_EXIT: u64 = 64;
/// `gettid() -> tid`: identifica a thread atual (não confundir com `getpid`).
pub const SYS_GETTID: u64 = 65;
/// `futex(&word, op, value, &timeout) -> result`: primitiva de espera/notificação.
/// Base de `Mutex`, `Condvar` e dos `park`/`unpark` da `std`. Ver `FUTEX_*`.
pub const SYS_FUTEX: u64 = 66;
/// `nanosleep(&Timespec, &Timespec) -> 0`: suspende a thread até o intervalo
/// esgotar. Interrompido por sinal devolve `-EINTR`.
pub const SYS_NANOSLEEP: u64 = 67;
/// `clock_gettime(clockid, &Timespec) -> 0`: relógio monótono (`CLOCK_MONOTONIC`)
/// ou de tempo real (`CLOCK_REALTIME`). Base de `Instant`/`SystemTime`.
pub const SYS_CLOCK_GETTIME: u64 = 68;
/// `mmap(address, length, protection, flags) -> address`: mapeia páginas de
/// usuário sob demanda. Devolve o endereço base ou um errno negativo.
pub const SYS_MMAP: u64 = 69;
/// `munmap(address, length) -> 0`: desmapeia páginas e devolve os frames ao
/// alocador. O par `mmap`/`munmap` é o que permite ao `GlobalAlloc` da `std`
/// crescer **e** liberar memória (o `sbrk` atual só cresce).
pub const SYS_MUNMAP: u64 = 70;
/// `set_tls(base) -> 0`: define a base do TLS da thread atual
/// (`IA32_FS_BASE`). É o equivalente do `arch_prctl(ARCH_SET_FS)` do Linux e
/// o que a musl chama em `__set_thread_area`; sem ele a thread principal não
/// tem `errno` por thread nem `pthread_self()`.
pub const SYS_SET_TLS: u64 = 71;
/// `tls_info(&TlsInfo) -> 0`: descreve o bloco TLS que o kernel montou para o
/// processo (`fs_base` e `memory_size`). A musl precisa disto para calcular
/// quanta memória cada thread nova consome; um `no_std` simples pode ignorar.
pub const SYS_TLS_INFO: u64 = 72;
}
pub use syscall::*;
}
