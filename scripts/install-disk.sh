#!/bin/sh
# ==============================================================================
# install-disk.sh — cria uma imagem de disco inicializável do ImagineOS
# ==============================================================================
#
# Layout GPT: ESP FAT16 (Limine) + partição DFS primária. O ESP carrega o
# contêiner `dzImage` publicado por este repositório: ele **é** o kernel e a
# raiz volátil, não há `ramfs.tar` separado. A DFS é semeada com o sistema
# (o `dfs-image` extrai o initramfs do próprio dzImage quando recebe só ele).
#
# Uso: install-disk.sh DISK-IMAGE DZIMAGE BOOTX64.EFI LIMINE.CONF STARTUP.NSH \
#                      DFS-TOOL [RAMFS-TAR-OPCIONAL]

set -eu

if [ "$#" -lt 6 ] || [ "$#" -gt 7 ]; then
    printf 'Usage: %s DISK-IMAGE DZIMAGE BOOTX64.EFI LIMINE.CONF STARTUP.NSH DFS-TOOL [RAMFS-TAR]\n' "$0" >&2
    exit 2
fi
disk_image=$1
dz_image=$2
bootx64=$3
limine_config=$4
startup_script=$5
dfs_tool=$6
ramfs=${7:-}

esp_image="${disk_image}.esp"
if [ -e "$disk_image" ] || [ -e "$esp_image" ]; then
    printf 'Refusing to overwrite an existing image or temporary ESP: %s\n' "$disk_image" >&2
    exit 1
fi
for required in "$dz_image" "$bootx64" "$limine_config" "$startup_script" "$dfs_tool"; do
    if [ ! -f "$required" ]; then
        printf 'Required installation file not found: %s\n' "$required" >&2
        exit 1
    fi
done
for tool in qemu-img sgdisk mkfs.vfat mmd mcopy dd; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'Required installation tool not found: %s\n' "$tool" >&2
        exit 1
    fi
done
# A raiz persistente (DFS) é semeada a partir do initramfs do dzImage, a não
# ser que um USTAR explícito seja passado.
if [ -z "$ramfs" ]; then
    ramfs="${disk_image}.initramfs.tar"
    "$dfs_tool" --dump-initramfs "$dz_image" "$ramfs"
fi

cleanup() {
    rm -f -- "$esp_image" "$ramfs"
}
trap cleanup EXIT HUP INT TERM

qemu-img create -f raw "$disk_image" 1G
sgdisk --align-end --clear \
    --new=1:2048:+128M --typecode=1:EF00 --change-name=1:IMAGINEOS_ESP \
    --new=2:0:0 --typecode=2:8A7F2C9D-6B31-4E52-9B14-445346530001 \
    --change-name=2:DREAMCORE_DFS "$disk_image"
dd if=/dev/zero of="$esp_image" bs=1M count=128 status=none
mkfs.vfat -F 16 -n IMAGINEOS "$esp_image"
mmd -i "$esp_image" ::/EFI ::/EFI/BOOT ::/boot
mcopy -i "$esp_image" "$bootx64" ::/EFI/BOOT/BOOTX64.EFI
mcopy -i "$esp_image" "$limine_config" ::/limine.conf
mcopy -i "$esp_image" "$startup_script" ::/startup.nsh
mcopy -i "$esp_image" "$dz_image" ::/boot/dzImage
dd if="$esp_image" of="$disk_image" bs=512 seek=2048 conv=notrunc status=none
"$dfs_tool" "$disk_image" "$ramfs"
printf 'Installed bootable GPT disk image: %s\n' "$disk_image"
printf 'EFI System Partition: 128 MiB FAT16 (Limine + dzImage); DFS seeded from the dzImage initramfs.\n'
