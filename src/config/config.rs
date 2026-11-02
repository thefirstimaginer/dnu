use core::sync::atomic::{AtomicU8, Ordering};

pub const GLOBAL_CONFIG_PATH: &str = "/home/.global/global.conf";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardLayout {
    Us,
    Abnt2,
}

static KEYBOARD_LAYOUT: AtomicU8 = AtomicU8::new(0);

pub fn keyboard_layout() -> KeyboardLayout {
    if KEYBOARD_LAYOUT.load(Ordering::Relaxed) == 1 {
        KeyboardLayout::Abnt2
    } else {
        KeyboardLayout::Us
    }
}

pub fn is_global_config_path(path: &str) -> bool {
    path.strip_prefix('/').unwrap_or(path) == GLOBAL_CONFIG_PATH.trim_start_matches('/')
}

pub fn validate(bytes: &[u8]) -> Result<(), ()> {
    parse(bytes).map(|_| ())
}

pub fn load(bytes: &[u8]) -> Result<(), ()> {
    let layout = parse(bytes)?;
    KEYBOARD_LAYOUT.store(
        match layout {
            KeyboardLayout::Us => 0,
            KeyboardLayout::Abnt2 => 1,
        },
        Ordering::Relaxed,
    );
    Ok(())
}

fn parse(bytes: &[u8]) -> Result<KeyboardLayout, ()> {
    let text = core::str::from_utf8(bytes).map_err(|_| ())?;
    let mut layout = KeyboardLayout::Us;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or(())?;
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return Err(());
        }
        match key {
            "charset" if value == "utf-8" => {}
            "charset" => return Err(()),
            "keyboard" => {
                layout = match value {
                    "us" => KeyboardLayout::Us,
                    "abnt2" => KeyboardLayout::Abnt2,
                    _ => return Err(()),
                };
            }
            _ => {}
        }
    }
    Ok(layout)
}

#[cfg(test)]
mod tests {
    use super::{load, validate, KeyboardLayout};

    #[test]
    fn parses_utf8_and_layout_settings() {
        assert_eq!(validate(b"charset=utf-8\nkeyboard=abnt2\n"), Ok(()));
        assert_eq!(load(b"charset=utf-8\nkeyboard=abnt2\n"), Ok(()));
        assert_eq!(super::keyboard_layout(), KeyboardLayout::Abnt2);
        assert_eq!(load(b"keyboard=us\n"), Ok(()));
        assert_eq!(super::keyboard_layout(), KeyboardLayout::Us);
    }

    #[test]
    fn rejects_unsupported_or_malformed_settings() {
        assert!(validate(b"charset=latin1\n").is_err());
        assert!(validate(b"keyboard=dvorak\n").is_err());
        assert!(validate(b"keyboard\n").is_err());
        assert!(validate(b"\xff").is_err());
    }
}
