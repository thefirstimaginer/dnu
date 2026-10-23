pub const MAGIC: &[u8; 8] = b"DZIMAGE\0";
pub const HEADER_SIZE: usize = 24;
pub const MAX_KERNEL_SIZE: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnpackError {
    InvalidHeader,
    InvalidSize,
    InvalidLz4,
    ChecksumMismatch,
}

pub fn unpacked_size(image: &[u8]) -> Result<usize, UnpackError> {
    if image.len() < HEADER_SIZE || &image[..8] != MAGIC || image[20..24] != [0; 4] {
        return Err(UnpackError::InvalidHeader);
    }
    let size = usize::try_from(read_u64(image, 8)?).map_err(|_| UnpackError::InvalidSize)?;
    if size == 0 || size > MAX_KERNEL_SIZE {
        return Err(UnpackError::InvalidSize);
    }
    Ok(size)
}

pub fn unpack(
    image: &[u8],
    output: &mut [u8],
    mut report_progress: impl FnMut(u8),
) -> Result<usize, UnpackError> {
    let output_size = unpacked_size(image)?;
    if output_size > output.len() {
        return Err(UnpackError::InvalidSize);
    }
    let expected_checksum = read_u32(image, 16)?;

    report_progress(0);
    let mut input = &image[HEADER_SIZE..];
    let mut written = 0usize;
    let mut last_progress = 0u8;
    while !input.is_empty() {
        let token = take_byte(&mut input)?;
        let literal_length = read_extended_length(&mut input, (token >> 4) as usize)?;
        let literal_end = written
            .checked_add(literal_length)
            .filter(|end| *end <= output_size)
            .ok_or(UnpackError::InvalidLz4)?;
        if input.len() < literal_length {
            return Err(UnpackError::InvalidLz4);
        }
        output[written..literal_end].copy_from_slice(&input[..literal_length]);
        input = &input[literal_length..];
        written = literal_end;
        report_new_progress(
            written,
            output_size,
            &mut last_progress,
            &mut report_progress,
        );
        if input.is_empty() {
            break;
        }

        let offset = take_u16(&mut input)? as usize;
        if offset == 0 || offset > written {
            return Err(UnpackError::InvalidLz4);
        }
        let match_length = read_extended_length(&mut input, (token & 0x0f) as usize)?
            .checked_add(4)
            .ok_or(UnpackError::InvalidLz4)?;
        let match_end = written
            .checked_add(match_length)
            .filter(|end| *end <= output_size)
            .ok_or(UnpackError::InvalidLz4)?;
        while written < match_end {
            output[written] = output[written - offset];
            written += 1;
            if written & 0x0fff == 0 {
                report_new_progress(
                    written,
                    output_size,
                    &mut last_progress,
                    &mut report_progress,
                );
            }
        }
        report_new_progress(
            written,
            output_size,
            &mut last_progress,
            &mut report_progress,
        );
    }
    if written != output_size {
        return Err(UnpackError::InvalidLz4);
    }
    if crc32(&output[..output_size]) != expected_checksum {
        return Err(UnpackError::ChecksumMismatch);
    }
    if last_progress != 100 {
        report_progress(100);
    }
    Ok(output_size)
}

fn report_new_progress(
    written: usize,
    total: usize,
    previous: &mut u8,
    report: &mut impl FnMut(u8),
) {
    let progress = ((written as u128 * 100) / total as u128) as u8;
    if progress > *previous {
        *previous = progress;
        report(progress);
    }
}

fn read_extended_length(input: &mut &[u8], initial: usize) -> Result<usize, UnpackError> {
    if initial != 15 {
        return Ok(initial);
    }
    let mut length = initial;
    loop {
        let byte = take_byte(input)? as usize;
        length = length.checked_add(byte).ok_or(UnpackError::InvalidLz4)?;
        if byte != 255 {
            return Ok(length);
        }
    }
}

fn take_byte(input: &mut &[u8]) -> Result<u8, UnpackError> {
    let (byte, remaining) = input.split_first().ok_or(UnpackError::InvalidLz4)?;
    *input = remaining;
    Ok(*byte)
}

fn take_u16(input: &mut &[u8]) -> Result<u16, UnpackError> {
    let bytes = input.get(..2).ok_or(UnpackError::InvalidLz4)?;
    *input = &input[2..];
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(input: &[u8], offset: usize) -> Result<u32, UnpackError> {
    let bytes = input
        .get(offset..offset + 4)
        .ok_or(UnpackError::InvalidHeader)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(input: &[u8], offset: usize) -> Result<u64, UnpackError> {
    let bytes = input
        .get(offset..offset + 8)
        .ok_or(UnpackError::InvalidHeader)?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut checksum = !0u32;
    for &byte in bytes {
        checksum ^= byte as u32;
        for _ in 0..8 {
            checksum = (checksum >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(checksum & 1)));
        }
    }
    !checksum
}

#[cfg(test)]
mod tests {
    use super::{crc32, unpack, UnpackError, HEADER_SIZE, MAGIC};

    fn image(payload: &[u8], unpacked: &[u8]) -> allocless::Image {
        let mut bytes = [0u8; 64];
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..16].copy_from_slice(&(unpacked.len() as u64).to_le_bytes());
        bytes[16..20].copy_from_slice(&crc32(unpacked).to_le_bytes());
        bytes[HEADER_SIZE..HEADER_SIZE + payload.len()].copy_from_slice(payload);
        allocless::Image {
            bytes,
            length: HEADER_SIZE + payload.len(),
        }
    }

    #[test]
    fn decompresses_overlapping_lz4_match_and_reports_progress() {
        let compressed = [0x35, b'a', b'b', b'c', 3, 0];
        let archive = image(&compressed, b"abcabcabcabc");
        let mut output = [0; 12];
        let mut last_progress = 0;
        let length = unpack(&archive.bytes[..archive.length], &mut output, |progress| {
            assert!(progress >= last_progress);
            last_progress = progress;
        })
        .unwrap();
        assert_eq!(&output[..length], b"abcabcabcabc");
        assert_eq!(last_progress, 100);
    }

    #[test]
    fn rejects_corrupt_header_checksum_and_truncated_streams() {
        let compressed = [0x40, b't', b'e', b's', b't'];
        let mut valid = image(&compressed, b"test");
        let mut output = [0; 4];
        valid.bytes[16] ^= 1;
        assert_eq!(
            unpack(&valid.bytes[..valid.length], &mut output, |_| {}),
            Err(UnpackError::ChecksumMismatch)
        );
        assert_eq!(
            unpack(&valid.bytes[..HEADER_SIZE], &mut output, |_| {}),
            Err(UnpackError::InvalidLz4)
        );
    }

    mod allocless {
        pub struct Image {
            pub bytes: [u8; 64],
            pub length: usize,
        }
    }
}
