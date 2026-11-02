import struct

# Read existing kernel ELF
with open('target/x86_64-unknown-none/release/imagineos-dnu-kernel', 'rb') as f:
    elf = bytearray(f.read())

# PVH note
# name = "pvh\0", descsz = 16 (2 QWORDs), type = 4 (NT_GNU_BUILD_ID)
note_name = b'pvh\0'
pvh_desc = struct.pack('<QQ', 2, 0)  # version 2, flags 0

namesz = len(pvh_name)
descsz = len(pvh_desc)

# Align to 8 bytes
namesz_aligned = (namesz + 3) & ~3
descsz_aligned = (descsz + 3) & ~3

# Note header: namesz(4), descsz(4), type(4) = 12 bytes
note_header = struct.pack('<III', namesz_aligned, descsz_aligned, 4)

note_size = 12 + namesz_aligned + descsz_aligned

# Create note
note = struct.pack('<III', namesz_aligned, descsz_aligned, 4)
note += pvh_name
note += b'\x00' * (namesz_aligned - namesz)
note += pvh_desc
note += b'\x00' * (descsz_aligned - descsz)

# Current ELF length
elf_size = len(elf)

# Build new ELF
# ELF header (64) + PHDR (56*2=112) + code + note
new_elf = bytearray(elf)
new_elf += note  # Append note at end

# Update e_shoff to point to new section header table location
# section header table starts right after all segments
new_elf_size = len(new_elf)

# Build section header for .note.pvh
sh_note_sec = bytearray(64)
struct.pack_into('<I', sh_note_sec, 0, 1)   # sh_name (index into strtab)
struct.pack_into('<I', sh_note_sec, 4, 7)   # sh_type = SHT_NOTE
struct.pack_into('<Q', sh_note_sec, 8, 2)   # sh_flags = SHF_ALLOC
struct.pack_into('<Q', sh_note_sec, 16, note_start)  # sh_addr
struct.pack_into('<Q', sh_note_sec, 24, note_start)  # sh_offset
struct.pack_into('<Q', sh_note_sec, 32, len(note_data))  # sh_size
struct.pack_into('<Q', sh_note_sec, 40, 0)   # sh_link
struct.pack_into('<Q', sh_note_sec, 48, 0)   # sh_info
struct.pack_into('<Q', sh_note_sec, 56, 1)   # sh_addralign = 1
struct.pack_into('<Q', sh_note_sec, 60, 0)   # sh_entsize = 0

# Append section header to end of ELF
new_elf += sh_note_sec

# Update e_shoff to point to new section header table
# e_shoff points to where section headers start
struct.pack_into('<Q', new_elf, 40, sh_offset)

# Update e_shnum (number of section headers)
struct.pack_into('<H', new_elf, 60, e_shnum + 1)

with open('build/kernel_pvh.elf', 'wb') as f:
    f.write(new_elf)

print(f'Kernel PVH: {len(new_elf)} bytes')
print(f'  e_shoff: {e_shoff}')
print(f'  e_shnum: {e_shnum} -> {e_shnum + 1}')
print(f'  Note offset: {note_start}')
print('Done!')