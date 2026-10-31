pub struct Decoder {
    codepoint: u32,
    minimum: u32,
    remaining: u8,
    pending_byte: Option<u8>,
}

impl Decoder {
    pub const fn new() -> Self {
        Self {
            codepoint: 0,
            minimum: 0,
            remaining: 0,
            pending_byte: None,
        }
    }

    pub fn push(&mut self, byte: u8) -> Option<char> {
        if self.remaining == 0 {
            return match byte {
                0x00..=0x7f => Some(byte as char),
                0xc2..=0xdf => {
                    self.codepoint = (byte & 0x1f) as u32;
                    self.minimum = 0x80;
                    self.remaining = 1;
                    None
                }
                0xe0..=0xef => {
                    self.codepoint = (byte & 0x0f) as u32;
                    self.minimum = 0x800;
                    self.remaining = 2;
                    None
                }
                0xf0..=0xf4 => {
                    self.codepoint = (byte & 0x07) as u32;
                    self.minimum = 0x10000;
                    self.remaining = 3;
                    None
                }
                _ => Some('\u{fffd}'),
            };
        }

        if byte & 0xc0 != 0x80 {
            *self = Self::new();
            self.pending_byte = Some(byte);
            return Some('\u{fffd}');
        }
        self.codepoint = (self.codepoint << 6) | (byte & 0x3f) as u32;
        self.remaining -= 1;
        if self.remaining != 0 {
            return None;
        }
        if self.codepoint < self.minimum {
            return Some('\u{fffd}');
        }
        char::from_u32(self.codepoint).or(Some('\u{fffd}'))
    }

    pub fn push_pending(&mut self) -> Option<char> {
        self.pending_byte.take().and_then(|byte| self.push(byte))
    }
}

#[cfg(test)]
mod tests {
    use super::Decoder;

    #[test]
    fn decodes_utf8_sequences_as_unicode_scalars() {
        let mut decoder = Decoder::new();
        assert_eq!(decoder.push(0xc3), None);
        assert_eq!(decoder.push(0xa7), Some('ç'));
        assert_eq!(decoder.push(0xc3), None);
        assert_eq!(decoder.push(0xa3), Some('ã'));
    }

    #[test]
    fn replaces_invalid_utf8_without_dropping_following_ascii() {
        let mut decoder = Decoder::new();
        assert_eq!(decoder.push(0xff), Some('\u{fffd}'));
        assert_eq!(decoder.push(b'A'), Some('A'));
        assert_eq!(decoder.push(0xc3), None);
        assert_eq!(decoder.push(b'B'), Some('\u{fffd}'));
        assert_eq!(decoder.push_pending(), Some('B'));
        assert_eq!(decoder.push(b'C'), Some('C'));
    }

    #[test]
    fn rejects_overlong_and_surrogate_encodings() {
        let mut decoder = Decoder::new();
        assert_eq!(decoder.push(0xc0), Some('\u{fffd}'));
        assert_eq!(decoder.push(0xe0), None);
        assert_eq!(decoder.push(0x80), None);
        assert_eq!(decoder.push(0x80), Some('\u{fffd}'));
        assert_eq!(decoder.push(0xed), None);
        assert_eq!(decoder.push(0xa0), None);
        assert_eq!(decoder.push(0x80), Some('\u{fffd}'));
    }
}
