#!/bin/sh
# ==============================================================================
# abi-bundle.sh — agrupa o crate `imagineos_abi` num único `.rs`
# ==============================================================================
#
# As ferramentas de host (`dnu-pack`, `dnulink`) são compiladas por `rustc`
# direto, sem cargo. O `.rlib` que o `build.rs` da ABI publica contém apenas
# metadados (o crate não gera código) e o `rustc` não o aceita como `--extern`
# de outro crate.
#
# Este script reproduz o agrupamento que o cargo faz: copia `lib.rs` (sem
# `#![no_std]`, que não é aceito dentro de um `mod`) e concatena os módulos
# declarados por ele, cada um dentro de `pub mod <nome> { ... }`.
#
# Uso:  abi-bundle.sh <raiz do crate> <saida.rs>

set -eu

root=$1
output=$2

mkdir -p "$(dirname "$output")"

{
    echo "// ==== Gerado por tools/abi-bundle.sh — não edite à mão. ===="
    echo "// Fonte: abi/imagineos_abi/src/*.rs"
    echo "pub mod imagineos_abi {"
    # Os módulos referenciam `crate::ABI_VERSION`; o `sed` final troca por
    # `ABI_VERSION`, resolvido neste nível do `mod imagineos_abi`.
    # `#![no_std]` é atributo interno de crate; dentro de `mod` ele não vale, e
    # o crate da ABI não usa a std de qualquer forma.
    sed -e '/^#!\[no_std\]/d' -e '/^pub mod [a-z_0-9]*;$/d' \
        -e '/^pub use [a-z_0-9]*::\*;$/d' "$root/src/lib.rs"
    for module in $(sed -n 's/^pub mod \([a-z_0-9]*\);/\1/p' "$root/src/lib.rs"); do
        echo "pub mod $module {"
        cat "$root/src/$module.rs"
        echo "}"
        # Reexporta como o `lib.rs` fazia (`pub use syscall::*;`).
        if grep -q "^pub use $module::\*;" "$root/src/lib.rs"; then
            echo "pub use $module::*;"
        fi
    done
    echo "}"
} > "$output"

# O crate original usava `crate::ABI_VERSION` dentro dos módulos; no bundle
# esses caminhos apontam para o aninhamento do crate consumidor, então o
# prefixo é removido (a constante está no mesmo `mod imagineos_abi`).
# Dentro de `imagineos_abi`, a raiz da biblioteca original fica um nível acima
# (`super::`): o consumidor aninha o bundle sob `crate::abi::imagineos_abi`.
sed -i 's/crate::/super::/g' "$output"
