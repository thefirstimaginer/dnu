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

/// Versão atual do formato DEF. Independente de [`crate::ABI_VERSION`]: o
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
        abi_version: crate::ABI_VERSION as u32,
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
///   [`crate::ABI_VERSION`]; a ferramenta passa a mesma para simular o loader);
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
