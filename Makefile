# ==============================================================================
# DNU - Dreamcore Kernel — build independente
# ==============================================================================
#
# Este repositório produz um único artefato publicado: `.build/dzImage`
# (DNU Zipped Image: kernel ELF + initramfs opcional). Toda a estrutura do
# ImagineOS (ISO, imagem de disco, diretório do instalador) consome só esse
# arquivo.
#
# Alvos:
#   dzImage     (padrão)  o único artefato final: kernel empacotado em dzImage
#   kernel                ELF do kernel, sem empacotar
#   kernel-direct         ELF do kernel para boot direto (feature direct-boot)
#   boot.bin              binário de boot direto (kernel + initramfs embutidos)
#   tools                 ferramentas de host (dzimage, dnudebug, dnutest, dnuboot)
#   test                  suíte de testes do kernel no host
#   check                 checagem de tipos do kernel no alvo bare-metal
#   clean                 remove .build/ e target/
#
# Variável de ambiente:
#   RAMFS_DIR   árvore que vira a raiz volátil (initramfs). O ImagineOS aponta
#               para a árvore `ramfs/` dele. Quando não informada, o dzImage
#               contém apenas o kernel.
#
# O kernel só compila em nightly (usa `#![feature]` do crate `limine`).
# ==============================================================================

ROOT := $(patsubst %/,%,$(dir $(abspath $(lastword $(MAKEFILE_LIST)))))
SHELL := /bin/bash

CARGO       := rustup run nightly cargo
CARGO_RUSTC := $(shell rustup run nightly which rustc)
OBJCOPY     := $(shell rustup run nightly which objcopy)
LD          := $(shell rustup run nightly which ld)

TARGET      := x86_64-unknown-none
HOST_TRIPLE := $(shell $(CARGO_RUSTC) -vV | sed -n 's/^host: //p')

RAMFS_DIR   ?=

BUILD_DIR     := $(ROOT)/.build
KERNEL        := $(ROOT)/target/$(TARGET)/release/imagineos-dnu-kernel
KERNEL_DIRECT := $(BUILD_DIR)/kernel-direct.elf
DZ_IMAGE      := $(BUILD_DIR)/dzImage
BOOT_BIN      := $(BUILD_DIR)/boot.bin
DZIMAGE_BIN   := $(BUILD_DIR)/tools/dzimage
DNUDEBUG_BIN  := $(BUILD_DIR)/tools/dnudebug
DNUTEST_BIN   := $(BUILD_DIR)/tools/dnutest
DNUBOOT_BIN   := $(BUILD_DIR)/tools/dnuboot

# Fonte PSF embarcada no kernel.
FONT := $(ROOT)/src/assets/zap-vga16.psf

# Versão da ABI, lida da definição canônica em src/abi/lib.rs.
ABI_VERSION := $(shell sed -n 's/^pub const ABI_VERSION: u64 = \([0-9][0-9]*\).*/\1/p' \
                    $(ROOT)/src/abi/lib.rs)

# Cada arquivo de tests/ é um harness `rustc --test` no host.
KERNEL_TESTS := $(patsubst $(ROOT)/tests/%.rs,%,$(wildcard $(ROOT)/tests/*.rs))

.PHONY: all dzImage kernel kernel-direct boot.bin tools test check clean help

all: dzImage

help:
	@sed -n '5,21p' $(lastword $(MAKEFILE_LIST)) | sed 's/^# \{0,1\}//'

# ------------------------------------------------------------------------------
# Kernel (ELF) — modo Limine (padrão)
# ------------------------------------------------------------------------------
kernel: $(KERNEL)

$(KERNEL): $(shell find $(ROOT)/src -name '*.rs') $(ROOT)/src/linker.ld $(FONT) $(ROOT)/Cargo.toml
	$(CARGO) build --release --target $(TARGET) --bin imagineos-dnu-kernel \
		--manifest-path $(ROOT)/Cargo.toml \
		--config 'target.x86_64-unknown-none.rustflags=["-C","link-arg=-Tsrc/linker.ld","-C","relocation-model=static"]'

# ------------------------------------------------------------------------------
# Kernel (ELF) — modo direto (dnuboot, sem Limine)
# ------------------------------------------------------------------------------
kernel-direct: $(KERNEL_DIRECT)

$(KERNEL_DIRECT): $(shell find $(ROOT)/src -name '*.rs') $(ROOT)/src/linker_direct.ld $(FONT) $(ROOT)/Cargo.toml
	@mkdir -p $(BUILD_DIR)
	$(CARGO) build --release --target $(TARGET) --bin imagineos-dnu-kernel \
		--features direct-boot \
		--manifest-path $(ROOT)/Cargo.toml \
		--config 'target.x86_64-unknown-none.rustflags=["-C","link-arg=-Tsrc/linker_direct.ld","-C","relocation-model=static"]'
	cp $(ROOT)/target/$(TARGET)/release/imagineos-dnu-kernel $(KERNEL_DIRECT)

# ------------------------------------------------------------------------------
# Ferramentas de host (dzimage, dnudebug, dnutest, dnuboot)
# ------------------------------------------------------------------------------
tools: $(DZIMAGE_BIN) $(DNUDEBUG_BIN) $(DNUTEST_BIN) $(DNUBOOT_BIN)

$(DZIMAGE_BIN): $(ROOT)/tools/dzimage_host.rs $(ROOT)/src/boot/dzimage.rs
	@mkdir -p $(BUILD_DIR)/tools
	$(CARGO_RUSTC) --edition 2021 --crate-type bin --crate-name dzimage \
		-C panic=abort $< -o $@

$(DNUDEBUG_BIN): $(ROOT)/tools/dnudebug.rs $(ROOT)/src/boot/dzimage.rs
	@mkdir -p $(BUILD_DIR)/tools
	$(CARGO_RUSTC) --edition 2021 --crate-type bin --crate-name dnudebug \
		-C panic=abort $< -o $@

$(DNUTEST_BIN): $(ROOT)/tools/dnutest.rs
	@mkdir -p $(BUILD_DIR)/tools
	$(CARGO_RUSTC) --edition 2021 --crate-type bin --crate-name dnutest \
		-C panic=abort $< -o $@

$(DNUBOOT_BIN): $(ROOT)/tools/dnuboot.rs $(ROOT)/src/boot/dzimage.rs
	@mkdir -p $(BUILD_DIR)/tools
	$(CARGO_RUSTC) --edition 2021 --crate-type bin --crate-name dnuboot \
		-C panic=abort $< -o $@

# ------------------------------------------------------------------------------
# Binário de boot direto (kernel direto, sem Limine)
# ------------------------------------------------------------------------------
boot.bin: $(KERNEL_DIRECT) $(OBJCOPY)
	@mkdir -p $(BUILD_DIR)
	# Converte o kernel ELF direto em binário bruto.
	$(OBJCOPY) -O binary $(KERNEL_DIRECT) $(BUILD_DIR)/kernel-direct.bin
	# Calcula o endereço de boot::direct_start e monta um stub de salto absoluto:
	#   mov dx, 0x3f8; mov al, 'X'; out dx, al   (marcador na serial)
	#   mov rax, <addr_direct_start>              = 48 c7 c0 <addr little-endian>
	#   jmp rax                                   = ff e0
	@python3 -c "import struct,subprocess; out=subprocess.check_output(['nm','$(KERNEL_DIRECT)']).decode(); sym=[l for l in out.splitlines() if 'direct_start' in l and l.split()[-1].endswith('direct_start')][0]; addr=sym.split()[0]; open('$(BUILD_DIR)/boot_stub.bin','wb').write(b'\xba\xf8\x03\xb0\x58\xee'+b'\x48\xc7\xc0'+struct.pack('<Q',int(addr,16))+b'\xff\xe0')"
	# Concatena: stub + kernel
	cat $(BUILD_DIR)/boot_stub.bin $(BUILD_DIR)/kernel-direct.bin > $(BOOT_BIN)

# ------------------------------------------------------------------------------
# Raiz volátil (opcional) e dzImage — o único artefato final
# ------------------------------------------------------------------------------
# O dzImage é empacotado com a ferramenta de host `dzimage`. Quando RAMFS_DIR
# é informado, monta-se um initramfs (tar USTAR) da árvore e o dzImage final
# contém kernel + initramfs. Caso contrário, contém apenas o kernel.
dzImage: $(DZ_IMAGE)

# Initramfs: tar da árvore RAMFS_DIR, se fornecida.
RAMFS_TAR := $(BUILD_DIR)/initramfs.tar
$(RAMFS_TAR): $(DZIMAGE_BIN)
	@mkdir -p $(BUILD_DIR)
	@rm -rf $(BUILD_DIR)/ramfs_stage
	@mkdir -p $(BUILD_DIR)/ramfs_stage
	if [ -n "$(RAMFS_DIR)" ]; then cp -a $(RAMFS_DIR)/. $(BUILD_DIR)/ramfs_stage/; fi
	mkdir -p $(BUILD_DIR)/ramfs_stage/etc $(BUILD_DIR)/ramfs_stage/home \
		$(BUILD_DIR)/ramfs_stage/tmp $(BUILD_DIR)/ramfs_stage/usr
	tar --format=ustar --numeric-owner --owner=0 --group=0 \
		-C $(BUILD_DIR)/ramfs_stage -cf $@ $$(cd $(BUILD_DIR)/ramfs_stage && find . -mindepth 1 -maxdepth 1 -printf '%P\n')

# Passo provisório: dzImage só com o kernel (payload do instalador).
.PHONY: provisional
provisional: $(DZIMAGE_BIN) $(KERNEL)
	@mkdir -p $(BUILD_DIR)
	$(DZIMAGE_BIN) pack --kernel $(KERNEL) --abi $(ABI_VERSION) -o $(DZ_IMAGE)

$(DZ_IMAGE): $(KERNEL) $(DZIMAGE_BIN) $(RAMFS_TAR) FORCE
	@if [ -f $(RAMFS_TAR) ]; then \
		$(DZIMAGE_BIN) pack --kernel $(KERNEL) --initramfs $(RAMFS_TAR) \
			--abi $(ABI_VERSION) -o $@; \
	else \
		$(DZIMAGE_BIN) pack --kernel $(KERNEL) --abi $(ABI_VERSION) -o $@; \
	fi

.PHONY: FORCE
FORCE:

check:
	$(CARGO) check --release --target $(TARGET) --bin imagineos-dnu-kernel \
		--manifest-path $(ROOT)/Cargo.toml \
		--config 'target.x86_64-unknown-none.rustflags=["-C","relocation-model=static"]'
