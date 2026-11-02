#!/bin/sh
# abi-bundle.sh — cria um bundle com os fontes da ABI para uso pelas ferramentas de host.
#
# Uso: abi-bundle.sh <abi-dir> <output-rs>
#
# Lê os arquivos .rs do diretório da ABI e gera um único arquivo Rust com todos
# os módulos embutidos como strings, para que as ferramentas de host possam
# compilar e verificar a ABI sem depender do crate do kernel.

set -eu

ABI_DIR="$1"
OUTPUT="$2"

if [ -z "$ABI_DIR" ] || [ -z "$OUTPUT" ]; then
    echo "Uso: $0 <abi-dir> <output-rs>" >&2
    exit 1
fi

if [ ! -d "$ABI_DIR" ]; then
    echo "Erro: diretório da ABI não encontrado: $ABI_DIR" >&2
    exit 1
fi

{
    echo "//! Bundle gerado automaticamente a partir da ABI em $ABI_DIR"
    echo "//! Não edite manualmente; regenere com tools/abi-bundle.sh"
    echo ""
    
    for f in "$ABI_DIR"/src/*.rs; do
        [ -f "$f" ] || continue
        echo "// ===== $f ====="
        cat "$f"
        echo ""
        echo "// ===== fim de $f ====="
        echo ""
    done
} > "$OUTPUT"

echo "Bundle da ABI gerado em $OUTPUT"