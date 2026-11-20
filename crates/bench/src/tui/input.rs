//! Pure ANSI/VT100 key sequence decoder for raw terminal input.

/// A decoded terminal keypress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Printable character (including `' '`).
    Char(char),
    /// Enter / Return (`\r` or `\n`).
    Enter,
    /// Standalone Escape (`\x1b`).
    Esc,
    /// Tab (`\t`).
    Tab,
    /// Shift-Tab (`\x1b[Z`).
    BackTab,
    /// Backspace (`\x7f` or `\x08`).
    Backspace,
    /// Delete (`\x1b[3~`).
    Delete,
    /// Arrow Up.
    Up,
    /// Arrow Down.
    Down,
    /// Arrow Left.
    Left,
    /// Arrow Right.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Function key F5 (`\x1b[15~`).
    F5,
    /// Ctrl-C (`\x03`).
    CtrlC,
    /// Ctrl-U (`\x15`) to clear an input line.
    CtrlU,
}

/// Decodes as many complete key events as possible from `buf`, returning the keys and the number
/// of consumed bytes. When `flush_incomplete` is `false`, a trailing partial escape sequence (such
/// as a lone `\x1b` that might be followed by `[A`) is left unconsumed so the caller can wait
/// briefly for the rest of the sequence; when `true`, it is emitted as [`Key::Esc`].
#[must_use]
pub fn parse_keys(buf: &[u8], flush_incomplete: bool) -> (Vec<Key>, usize) {
    let mut keys = Vec::new();
    let mut index = 0;
    while index < buf.len() {
        let byte = buf[index];
        if byte == 0x1b {
            match decode_escape(&buf[index..], flush_incomplete) {
                Some((Some(key), used)) => {
                    keys.push(key);
                    index += used;
                }
                Some((None, used)) => {
                    index += used;
                }
                None => break,
            }
            continue;
        }
        match byte {
            0x03 => {
                keys.push(Key::CtrlC);
                index += 1;
            }
            0x15 => {
                keys.push(Key::CtrlU);
                index += 1;
            }
            b'\r' | b'\n' => {
                keys.push(Key::Enter);
                index += 1;
            }
            b'\t' => {
                keys.push(Key::Tab);
                index += 1;
            }
            0x7f | 0x08 => {
                keys.push(Key::Backspace);
                index += 1;
            }
            0x00..=0x1f => {
                index += 1;
            }
            _ => match decode_utf8_char(&buf[index..], flush_incomplete) {
                Some((Some(ch), used)) => {
                    if !ch.is_control() {
                        keys.push(Key::Char(ch));
                    }
                    index += used;
                }
                Some((None, used)) => {
                    index += used;
                }
                None => break,
            },
        }
    }
    (keys, index)
}

fn decode_escape(slice: &[u8], flush_incomplete: bool) -> Option<(Option<Key>, usize)> {
    if slice.len() == 1 {
        return flush_incomplete.then_some((Some(Key::Esc), 1));
    }
    match slice[1] {
        b'[' => decode_csi(slice, flush_incomplete),
        b'O' => decode_ss3(slice, flush_incomplete),
        _ => Some((Some(Key::Esc), 1)),
    }
}

fn decode_ss3(slice: &[u8], flush_incomplete: bool) -> Option<(Option<Key>, usize)> {
    if slice.len() < 3 {
        return flush_incomplete.then_some((Some(Key::Esc), 1));
    }
    let key = match slice[2] {
        b'A' => Some(Key::Up),
        b'B' => Some(Key::Down),
        b'C' => Some(Key::Right),
        b'D' => Some(Key::Left),
        b'H' => Some(Key::Home),
        b'F' => Some(Key::End),
        _ => None,
    };
    Some((key, 3))
}

fn decode_csi(slice: &[u8], flush_incomplete: bool) -> Option<(Option<Key>, usize)> {
    if slice.len() < 3 {
        return flush_incomplete.then_some((Some(Key::Esc), 1));
    }
    let mut end = 2;
    while end < slice.len() && !(0x40..=0x7e).contains(&slice[end]) {
        end += 1;
        if end - 2 > 16 {
            return Some((None, end));
        }
    }
    if end >= slice.len() {
        return flush_incomplete.then_some((None, slice.len()));
    }
    let params = &slice[2..end];
    let final_byte = slice[end];
    let used = end + 1;
    let first_num = std::str::from_utf8(params)
        .ok()
        .and_then(|raw| raw.split(';').next())
        .and_then(|part| part.parse::<u16>().ok());
    let key = match final_byte {
        b'A' => Some(Key::Up),
        b'B' => Some(Key::Down),
        b'C' => Some(Key::Right),
        b'D' => Some(Key::Left),
        b'H' => Some(Key::Home),
        b'F' => Some(Key::End),
        b'Z' => Some(Key::BackTab),
        b'~' => match first_num {
            Some(1 | 7) => Some(Key::Home),
            Some(3) => Some(Key::Delete),
            Some(4 | 8) => Some(Key::End),
            Some(5) => Some(Key::PageUp),
            Some(6) => Some(Key::PageDown),
            Some(15) => Some(Key::F5),
            _ => None,
        },
        _ => None,
    };
    Some((key, used))
}

fn decode_utf8_char(slice: &[u8], flush_incomplete: bool) -> Option<(Option<char>, usize)> {
    match std::str::from_utf8(slice) {
        Ok(valid) => {
            let ch = valid.chars().next()?;
            Some((Some(ch), ch.len_utf8()))
        }
        Err(err) => {
            let valid_len = err.valid_up_to();
            if valid_len > 0 {
                let valid = std::str::from_utf8(&slice[..valid_len]).ok()?;
                let ch = valid.chars().next()?;
                return Some((Some(ch), ch.len_utf8()));
            }
            match err.error_len() {
                Some(bad) => Some((None, bad)),
                None if flush_incomplete => Some((None, slice.len())),
                None => None,
            }
        }
    }
}
