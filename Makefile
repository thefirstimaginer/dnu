# ==============================================================================
# DNU - Dreamcore Kernel — build independente
# ==============================================================================
#
# Este repositório produz um único artefato: `.build/dzImage` (DNU Zipped
# Image). Toda a estrutura do ImagineOS (ISO, imagem de disco, diretório do
# instalador) consome só esse arquivo.
#
# Alvos:
#   dzImage     (padrão)  empacota o kernel e a raiz volátil num dzImage
#   kernel                ELF do kernel, sem empacotar
#   tools                 dzimage, dnu-pack, dnulink e dfs-image (host)
#   test                  ferramentas de host + suíte de testes do kernel
#   check                 checagem de tipos do kernel no alvo bare-metal
#   clean                 remove .build/
#
# Variável de ambiente:
#   RAMFS_DIR   árvore que vira a raiz volátil (padrão: vazia). O ImagineOS
#               aponta para a árvore `ramfs/` dele.
#
ROOT := $(patsubst %/,%,$(dir $(abspath $(lastword $(MAKEFILE_LIST)))))
SHELL := /bin/sh

# O crate `limine` usa `#![feature]`: o kernel só compila em nightly.
CARGO       := rustup run nightly cargo
CARGO_RUSTC := $(shell rustup run nightly which rustc)
AR          ?= ar

TARGET      := x86_64-unknown-none
HOST_TRIPLE := $(shell $(CARGO_RUSTC) -vV | sed -n 's/^host: //p')

# Directories with shell-escaped spaces are not supported; keep the tree path
# simple so recursive $(MAKE) invocations see the same value.
RAMFS_DIR   ?=

BUILD_DIR     := $(ROOT)/.build
KERNEL        := $(ROOT)/target/$(TARGET)/release/imagineos-dnu-kernel
DZ_IMAGE      := $(BUILD_DIR)/dzImage
DZIMAGE_BIN   := $(BUILD_DIR)/dzimage
DNU_PACK_BIN  := $(BUILD_DIR)/dnu-pack
DNULINK_BIN   := $(BUILD_DIR)/dnulink
DFS_IMAGE_BIN := $(BUILD_DIR)/dfs-image

# Raiz volátil empacotada.
RAMFS_TREE      := $(BUILD_DIR)/ramfs
RAMFS_INSTALLED := $(BUILD_DIR)/initramfs-installed.tar

# ABI (dona do formato DEF). O kernel a consome como crate do workspace; as
# ferramentas de host, compiladas por `rustc` direto, a enxergam pelo bundle
# gerado por `tools/abi-bundle.sh` (ver `tools/abi.rs`).
ABI_MANIFEST := $(ROOT)/abi/imagineos_abi/Cargo.toml
ABI_BUNDLE   := $(BUILD_DIR)/tools/imagineos_abi_bundle.rs
ABI_VERSION  := $(shell sed -n 's/^pub const ABI_VERSION: u64 = \([0-9][0-9]*\).*/\1/p' \
                    $(ROOT)/abi/imagineos_abi/src/lib.rs)

# O kernel é uma árvore plana em src/ (cada módulo-hub declara suas folhas).
KERNEL_SOURCES := $(shell find $(ROOT)/src $(ROOT)/abi/imagineos_abi/src -name '*.rs' 2>/dev/null)
FONT := $(ROOT)/src/assets/zap-vga16.psf

# Cada arquivo de tests/ é um harness `rustc --test` no host.
KERNEL_TESTS := $(patsubst $(ROOT)/tests/%.rs,%,$(wildcard $(ROOT)/tests/*.rs))

.PHONY: all dzImage kernel tools abi provisional test check clean help

all: dzImage

help:
	@sed -n '3,19p' $(lastword $(MAKEFILE_LIST)) | sed 's/^# \{0,1\}//'

# ------------------------------------------------------------------------------
# Kernel (ELF)
# ------------------------------------------------------------------------------
kernel: $(KERNEL)

$(KERNEL): $(KERNEL_SOURCES) $(ROOT)/src/linker.ld $(FONT) $(ROOT)/Cargo.toml
	$(CARGO) build --release --target $(TARGET) --bin imagineos-dnu-kernel \
		--manifest-path $(ROOT)/Cargo.toml \
		--config 'target.x86_64-unknown-none.rustflags=["-C","link-arg=-Tsrc/linker.ld","-C","relocation-model=static"]'

check:
	$(CARGO) check --release --target $(TARGET) --bin imagineos-dnu-kernel \
		--manifest-path $(ROOT)/Cargo.toml \
		--config 'target.x86_64-unknown-none.rustflags=["-C","relocation-model=static"]'

# ------------------------------------------------------------------------------
# Ferramentas de host
# ------------------------------------------------------------------------------
tools: abi $(DZIMAGE_BIN) $(DNU_PACK_BIN) $(DNULINK_BIN) $(DFS_IMAGE_BIN)

$(DZIMAGE_BIN): $(ROOT)/dzimage.rs
	@mkdir -p $(BUILD_DIR)
	$(CARGO_RUSTC) --edition 2021 -C panic=abort $< -o $@

# O `dfs-image` inclui a leitura do dzImage (`read_entries`) para semear o
# disco a partir do próprio contêiner, sem um initramfs paralelo. Ele define os
# módulos `block`, `dfs`, `gpt` e `dzimage` como módulos da raiz da crate, então
# estes arquivos são compilados como `crate::<modulo>` — sem `mod.rs` nem
# `lib.rs` nesta árvore plana.
$(DFS_IMAGE_BIN): $(ROOT)/tools/dfs-image.rs $(ROOT)/src/block.rs \
                  $(ROOT)/src/dfs.rs $(ROOT)/src/gpt.rs $(ROOT)/src/dzimage.rs
	@mkdir -p $(BUILD_DIR)
	$(CARGO_RUSTC) --edition 2021 --crate-name dfs_image $< -o $@

# A validação da ABI é o `cargo check` do próprio crate no host, que compila
# `lib.rs` e os módulos como o kernel os vê.
abi: $(ABI_MANIFEST) $(wildcard $(ROOT)/abi/imagineos_abi/src/*.rs)
	@mkdir -p $(BUILD_DIR)/tools
	$(ROOT)/tools/abi-bundle.sh $(ROOT)/abi/imagineos_abi $(ABI_BUNDLE)
	$(CARGO) check --manifest-path $(ABI_MANIFEST) --lib --target $(HOST_TRIPLE) \
		--config 'profile.release.panic="unwind"'

$(ABI_BUNDLE): abi

# As ferramentas de host ligam o bundle da ABI junto das próprias fontes.
$(DNU_PACK_BIN): $(ROOT)/tools/dnu-pack.rs $(ABI_BUNDLE)
	@mkdir -p $(BUILD_DIR)
	OUT_DIR=$(BUILD_DIR)/tools $(CARGO_RUSTC) --crate-name dnu_pack \
		--edition 2024 $< -o $@

$(DNULINK_BIN): $(ROOT)/tools/dnulink.rs $(ABI_BUNDLE)
	@mkdir -p $(BUILD_DIR)
	OUT_DIR=$(BUILD_DIR)/tools $(CARGO_RUSTC) --crate-name dnulink \
		--edition 2024 $< -o $@

# ------------------------------------------------------------------------------
# Raiz volátil e dzImage
# ------------------------------------------------------------------------------
# `/system/install/dzImage` é o próprio arquivo instalado; por isso a raiz
# volátil é montada em duas passadas: o alvo `provisional` empacota um dzImage
# só com o kernel, o initramfs o embute e o dzImage final substitui o arquivo
# com o initramfs completo dentro.
dzImage: $(DZ_IMAGE)

provisional: $(DZIMAGE_BIN) $(KERNEL)
	@mkdir -p $(BUILD_DIR)
	$(DZIMAGE_BIN) pack --kernel $(KERNEL) --abi $(ABI_VERSION) -o $(DZ_IMAGE)

$(DZ_IMAGE): $(KERNEL) $(RAMFS_INSTALLED) $(DZIMAGE_BIN) FORCE
	$(DZIMAGE_BIN) pack --kernel $(KERNEL) --initramfs $(RAMFS_INSTALLED) \
		--abi $(ABI_VERSION) -o $@

# `FORCE`: o dzImage provisório pode já estar "atualizado" em relação ao
# kernel, e `$(RAMFS_INSTALLED)` reaproveitado não o torna mais novo. Sempre
# reempacotar custa uma cópia do ELF e garante que o arquivo final contenha o
# initramfs. O alvo `provisional` continua existindo para o payload do
# instalador.
.PHONY: FORCE
FORCE:
$(RAMFS_INSTALLED): $(DFS_IMAGE_BIN) $(DNU_PACK_BIN) $(DNULINK_BIN) \
                    $(shell find $(RAMFS_DIR) -type f 2>/dev/null)
	# Primeira passada: um dzImage com o kernel, para o payload do instalador.
	$(MAKE) -f $(ROOT)/Makefile provisional
	@rm -rf $(RAMFS_TREE)
	@mkdir -p $(RAMFS_TREE)
	if [ -n "$(RAMFS_DIR)" ]; then cp -a $(RAMFS_DIR)/. $(RAMFS_TREE)/; fi
	mkdir -p $(RAMFS_TREE)/etc $(RAMFS_TREE)/home $(RAMFS_TREE)/tmp \
		$(RAMFS_TREE)/system/install $(RAMFS_TREE)/system/toolchain $(RAMFS_TREE)/usr
	# O contêiner DEF do toolchain fica com o artefato publicado: uma máquina
	# ImagineOS instalada reempacota programas sem o repositório do kernel.
	cp $(DNU_PACK_BIN) $(RAMFS_TREE)/system/toolchain/dnu-pack
	cp $(DNULINK_BIN) $(RAMFS_TREE)/system/toolchain/dnulink
	chmod 0755 $(RAMFS_TREE)/system/toolchain/*
	cp $(DZ_IMAGE) $(RAMFS_TREE)/system/install/dzImage
	tar --format=ustar --numeric-owner --owner=0 --group=0 \
		-C $(RAMFS_TREE) -cf $@ $$(cd $(RAMFS_TREE) && find . -mindepth 1 -maxdepth 1 -printf '%P\n')

# ------------------------------------------------------------------------------
# Testes
# ------------------------------------------------------------------------------
# O kernel bare-metal não tem harness de teste: os arquivos de tests/ rodam no
# host, um `rustc --test` por arquivo (o mesmo fluxo da suíte histórica).
#
# Os harnesses compartilham o estado global do VFS (raiz montada, contas), então
# rodam em série (`--test-threads=1`): em paralelo dois testes disputam a mesma
# raiz e falham de forma intermitente.
test: tools
	@mkdir -p $(BUILD_DIR)/tests
	@set -e; \
	if [ "$(HOST_TRIPLE)" != "$(TARGET)" ]; then \
		abi_flags="--target $(HOST_TRIPLE) --edition 2021"; \
	else abi_flags=""; fi; \
	for harness in $(KERNEL_TESTS); do \
		out=$(BUILD_DIR)/tests/$$(echo $$harness | tr '/' '-'); \
		echo "== tests/$$harness"; \
		OUT_DIR=$(BUILD_DIR)/tools $(CARGO_RUSTC) --test \
			$(ROOT)/tests/$$harness.rs -o $$out $$abi_flags; \
		$$out --test-threads=1; \
	done
	@echo "== dzimage (host)"
	$(CARGO_RUSTC) --edition 2021 --test $(ROOT)/dzimage.rs \
		-o $(BUILD_DIR)/tests/dzimage-host
	$(BUILD_DIR)/tests/dzimage-host --test-threads=1

clean:
	rm -rf $(BUILD_DIR)
