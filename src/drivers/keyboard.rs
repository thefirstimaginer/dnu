use core::arch::asm;

use crate::config::KeyboardLayout;

const KEY_UP: char = '\u{f0000}';
const KEY_DOWN: char = '\u{f0001}';
const KEY_LEFT: char = '\u{f0002}';
const KEY_RIGHT: char = '\u{f0003}';
const KEY_HOME: char = '\u{f0004}';
const KEY_END: char = '\u{f0005}';
const KEY_DELETE: char = '\u{f0006}';
pub const KEY_INTERRUPT: char = '\u{f0007}';

pub struct Keyboard {
    left_shift: bool,
    right_shift: bool,
    control: bool,
    caps_lock: bool,
    extended: bool,
    alt_gr: bool,
    dead_key: Option<char>,
    pending: [char; 2],
    pending_length: usize,
    pending_index: usize,
}

impl Keyboard {
    pub const fn new() -> Self {
        Self {
            left_shift: false,
            right_shift: false,
            control: false,
            caps_lock: false,
            extended: false,
            alt_gr: false,
            dead_key: None,
            pending: ['\0'; 2],
            pending_length: 0,
            pending_index: 0,
        }
    }

    pub fn poll_char(&mut self) -> Option<char> {
        if self.pending_index < self.pending_length {
            let character = self.pending[self.pending_index];
            self.pending_index += 1;
            return Some(character);
        }
        self.pending_length = 0;
        self.pending_index = 0;

        unsafe {
            if in_port(0x64) & 1 == 0 {
                return None;
            }
            let code = in_port(0x60);
            if code == 0xe0 {
                self.extended = true;
                return None;
            }

            let released = code & 0x80 != 0;
            let key = code & 0x7f;
            let extended = core::mem::replace(&mut self.extended, false);
            match (extended, key) {
                (false, 0x2a) => self.left_shift = !released,
                (false, 0x36) => self.right_shift = !released,
                (false, 0x1d) | (true, 0x1d) => self.control = !released,
                (false, 0x3a) if !released => self.caps_lock = !self.caps_lock,
                (true, 0x38) => self.alt_gr = !released,
                _ => {}
            }
            if released {
                return None;
            }
            if extended {
                return extended_key(key);
            }
            if self.control && key == 0x2e {
                return Some(KEY_INTERRUPT);
            }

            let layout = crate::config::keyboard_layout();
            let character = translate(
                key,
                self.left_shift || self.right_shift,
                self.caps_lock,
                self.alt_gr,
                layout,
            )?;
            if layout == KeyboardLayout::Abnt2 {
                self.compose(character)
            } else {
                Some(character)
            }
        }
    }

    fn compose(&mut self, character: char) -> Option<char> {
        if is_dead_key(character) {
            self.dead_key = Some(character);
            return None;
        }
        let Some(dead_key) = self.dead_key.take() else {
            return Some(character);
        };
        if character == ' ' {
            return Some(dead_key);
        }
        if let Some(composed) = compose_dead_key(dead_key, character) {
            return Some(composed);
        }
        self.pending[0] = character;
        self.pending_length = 1;
        Some(dead_key)
    }
}

fn extended_key(code: u8) -> Option<char> {
    match code {
        0x48 => Some(KEY_UP),
        0x50 => Some(KEY_DOWN),
        0x4b => Some(KEY_LEFT),
        0x4d => Some(KEY_RIGHT),
        0x47 => Some(KEY_HOME),
        0x4f => Some(KEY_END),
        0x53 => Some(KEY_DELETE),
        _ => None,
    }
}

fn translate(
    code: u8,
    shift: bool,
    caps: bool,
    alt_gr: bool,
    layout: KeyboardLayout,
) -> Option<char> {
    if layout == KeyboardLayout::Abnt2 {
        return translate_abnt2(code, shift, caps, alt_gr);
    }
    translate_us(code, shift, caps, alt_gr)
}

fn translate_us(code: u8, shift: bool, caps: bool, alt_gr: bool) -> Option<char> {
    if alt_gr {
        return None;
    }
    translate_common(code, shift, caps)
}

fn translate_abnt2(code: u8, shift: bool, caps: bool, alt_gr: bool) -> Option<char> {
    if alt_gr {
        return match code {
            0x03 => Some('@'),
            0x04 => Some('£'),
            0x05 => Some('¢'),
            0x07 => Some('¬'),
            0x10 => Some('/'),
            0x11 => Some('?'),
            0x12 => Some('°'),
            0x56 => Some('|'),
            _ => None,
        };
    }
    match code {
        0x07 if shift => Some('¨'),
        0x1a => Some(if shift { '`' } else { '´' }),
        0x1b => Some(if shift { '{' } else { '[' }),
        0x27 => Some(if shift { 'Ç' } else { 'ç' }),
        0x28 => Some(if shift { '^' } else { '~' }),
        0x29 => Some(if shift { '"' } else { '\'' }),
        0x2b => Some(if shift { '}' } else { ']' }),
        0x56 => Some(if shift { '>' } else { '<' }),
        0x0c => Some(if shift { '_' } else { '-' }),
        0x0d => Some(if shift { '+' } else { '=' }),
        _ => translate_common(code, shift, caps),
    }
}

fn translate_common(code: u8, shift: bool, caps: bool) -> Option<char> {
    let character = match code {
        0x02..=0x0b => {
            const NORMAL: &[u8] = b"1234567890";
            const UPPER: &[u8] = b"!@#$%^&*()";
            if shift {
                UPPER[(code - 0x02) as usize]
            } else {
                NORMAL[(code - 0x02) as usize]
            }
        }
        0x10..=0x19 => {
            (if shift ^ caps {
                b"QWERTYUIOP"
            } else {
                b"qwertyuiop"
            })[(code - 0x10) as usize]
        }
        0x1e..=0x26 => {
            (if shift ^ caps {
                b"ASDFGHJKL"
            } else {
                b"asdfghjkl"
            })[(code - 0x1e) as usize]
        }
        0x2c..=0x32 => (if shift ^ caps { b"ZXCVBNM" } else { b"zxcvbnm" })[(code - 0x2c) as usize],
        0x1c => return Some('\n'),
        0x0e => return Some('\u{8}'),
        0x39 => return Some(' '),
        0x0c => {
            if shift {
                b'_'
            } else {
                b'-'
            }
        }
        0x0d => {
            if shift {
                b'+'
            } else {
                b'='
            }
        }
        0x1a => {
            if shift {
                b'{'
            } else {
                b'['
            }
        }
        0x1b => {
            if shift {
                b'}'
            } else {
                b']'
            }
        }
        0x27 => {
            if shift {
                b':'
            } else {
                b';'
            }
        }
        0x28 => {
            if shift {
                b'"'
            } else {
                b'\''
            }
        }
        0x29 => {
            if shift {
                b'~'
            } else {
                b'`'
            }
        }
        0x2b => {
            if shift {
                b'|'
            } else {
                b'\\'
            }
        }
        0x33 => {
            if shift {
                b'<'
            } else {
                b','
            }
        }
        0x34 => {
            if shift {
                b'>'
            } else {
                b'.'
            }
        }
        0x35 => {
            if shift {
                b'?'
            } else {
                b'/'
            }
        }
        _ => return None,
    };
    Some(character as char)
}

fn is_dead_key(character: char) -> bool {
    matches!(character, '´' | '`' | '~' | '^' | '¨')
}

fn compose_dead_key(dead_key: char, character: char) -> Option<char> {
    Some(match (dead_key, character) {
        ('´', 'a') => 'á',
        ('´', 'e') => 'é',
        ('´', 'i') => 'í',
        ('´', 'o') => 'ó',
        ('´', 'u') => 'ú',
        ('´', 'A') => 'Á',
        ('´', 'E') => 'É',
        ('´', 'I') => 'Í',
        ('´', 'O') => 'Ó',
        ('´', 'U') => 'Ú',
        ('`', 'a') => 'à',
        ('`', 'e') => 'è',
        ('`', 'i') => 'ì',
        ('`', 'o') => 'ò',
        ('`', 'u') => 'ù',
        ('`', 'A') => 'À',
        ('`', 'E') => 'È',
        ('`', 'I') => 'Ì',
        ('`', 'O') => 'Ò',
        ('`', 'U') => 'Ù',
        ('~', 'a') => 'ã',
        ('~', 'o') => 'õ',
        ('~', 'A') => 'Ã',
        ('~', 'O') => 'Õ',
        ('^', 'a') => 'â',
        ('^', 'e') => 'ê',
        ('^', 'o') => 'ô',
        ('^', 'A') => 'Â',
        ('^', 'E') => 'Ê',
        ('^', 'O') => 'Ô',
        ('¨', 'u') => 'ü',
        ('¨', 'U') => 'Ü',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{compose_dead_key, translate};
    use crate::config::KeyboardLayout;

    #[test]
    fn abnt2_maps_portuguese_keys_and_dead_key_compositions() {
        assert_eq!(
            translate(0x27, false, false, false, KeyboardLayout::Abnt2),
            Some('ç')
        );
        assert_eq!(
            translate(0x27, true, false, false, KeyboardLayout::Abnt2),
            Some('Ç')
        );
        assert_eq!(
            translate(0x03, false, false, true, KeyboardLayout::Abnt2),
            Some('@')
        );
        assert_eq!(
            translate(0x1a, false, false, false, KeyboardLayout::Abnt2),
            Some('´')
        );
        assert_eq!(
            translate(0x07, true, false, false, KeyboardLayout::Abnt2),
            Some('¨')
        );
        assert_eq!(compose_dead_key('´', 'a'), Some('á'));
        assert_eq!(compose_dead_key('~', 'o'), Some('õ'));
        assert_eq!(compose_dead_key('^', 'e'), Some('ê'));
    }

    #[test]
    fn extended_arrows_are_available_to_user_programs() {
        assert_eq!(super::extended_key(0x48), Some('\u{f0000}'));
        assert_eq!(super::extended_key(0x4d), Some('\u{f0003}'));
    }
}

unsafe fn in_port(port: u16) -> u8 {
    let value: u8;
    asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nomem, nostack, preserves_flags)
    );
    value
}
