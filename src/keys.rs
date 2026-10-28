//! Keys: Kel's own shortcuts, and the bytes every other key sends to the shell.
//!
//! When a key is pressed, `App::on_key` asks `shortcut` first. If it's one of
//! Kel's shortcuts, Kel runs the `Action`; if not, `key_to_bytes` turns the key
//! into bytes for the shell. Nothing here knows about windows or tabs, so it
//! can all be tested with plain values.

use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};

/// Something Kel does when you press one of its shortcuts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    NewTab,
    CloseTab,
    NextTab,
    PreviousTab,
    /// Tab 1…8, counted from 1 like the keys.
    GoToTab(usize),
    LastTab,
    Paste,
    /// Copies the mouse selection; without one, acts as `Interrupt`.
    Copy,
    /// Ctrl+C: stop the running command.
    Interrupt,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    OpenConfig,
}

/// Which of Kel's shortcuts this key is, if any. All of Kel's shortcuts, in one table.
///
/// Uses the physical key (its place on the keyboard), so shortcuts work with
/// any layout, Cyrillic too.
pub fn shortcut(key: PhysicalKey, mods: ModifiersState) -> Option<Action> {
    // Every shortcut uses Ctrl. Not with Alt: Ctrl+Alt is AltGr on many layouts,
    // and AltGr types letters (Polish AltGr+C is `ć`, not a Ctrl+C).
    if !mods.control_key() || mods.alt_key() {
        return None;
    }
    let PhysicalKey::Code(code) = key else {
        return None;
    };
    let action = match (code, mods.shift_key()) {
        // (key, with Shift?)
        (KeyCode::KeyT, true) => Action::NewTab,
        (KeyCode::KeyW, true) => Action::CloseTab,
        (KeyCode::Tab, false) => Action::NextTab,
        (KeyCode::Tab, true) => Action::PreviousTab,
        (KeyCode::Digit1, false) => Action::GoToTab(1),
        (KeyCode::Digit2, false) => Action::GoToTab(2),
        (KeyCode::Digit3, false) => Action::GoToTab(3),
        (KeyCode::Digit4, false) => Action::GoToTab(4),
        (KeyCode::Digit5, false) => Action::GoToTab(5),
        (KeyCode::Digit6, false) => Action::GoToTab(6),
        (KeyCode::Digit7, false) => Action::GoToTab(7),
        (KeyCode::Digit8, false) => Action::GoToTab(8),
        (KeyCode::Digit9, false) => Action::LastTab, // like browsers
        (KeyCode::KeyV, true) => Action::Paste,
        (KeyCode::KeyC, true) => Action::Copy,
        (KeyCode::KeyC, false) => Action::Interrupt,
        // With or without Shift: Ctrl+Shift+= is Ctrl++.
        (KeyCode::Equal, _) => Action::ZoomIn,
        (KeyCode::Minus, _) => Action::ZoomOut,
        (KeyCode::Digit0, _) => Action::ZoomReset,
        (KeyCode::Comma, _) => Action::OpenConfig,
        _ => return None,
    };
    Some(action)
}

/// The bytes to send for pasted text.
/// - Every line break becomes `\r`, because the shell treats `\r` as Enter.
/// - With bracketed paste on, the text goes between `ESC[200~` and `ESC[201~`, so the
///   program knows it's pasted, not typed: it won't run the lines or indent them.
///   An end marker hidden inside the text is removed, so it can't end the paste early.
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        let text = text.replace("\x1b[201~", "");
        format!("\x1b[200~{text}\x1b[201~").into_bytes()
    } else {
        text.into_bytes()
    }
}

/// Translates a key press into the bytes a terminal would send to the shell.
/// `None` means the key sends nothing (e.g. Shift alone).
pub fn key_to_bytes(key: &Key, text: Option<&str>, mods: ModifiersState) -> Option<Vec<u8>> {
    // Modifier number: 1 + Shift(1) + Alt(2) + Ctrl(4)
    // e.g. Ctrl+Left: m = 1 + 4 = 5, so it sends `ESC[1;5D`.
    let m = 1 + mods.shift_key() as u8 + 2 * mods.alt_key() as u8 + 4 * mods.control_key() as u8;

    // Arrows, Home and End: `\x1b[X` without modifiers, `\x1b[1;{m}X` with them.
    let with_mods = |letter: char| -> Option<Vec<u8>> {
        let seq = if m == 1 {
            format!("\x1b[{letter}")
        } else {
            format!("\x1b[1;{m}{letter}")
        };
        Some(seq.into_bytes())
    };

    // F1-F4: like arrows, but `\x1bOX` without modifiers (a leftover from the
    // PF1-PF4 keys of the VT100 terminal, which used `ESC O`).
    let f1_to_f4 = |letter: char| -> Option<Vec<u8>> {
        let seq = if m == 1 {
            format!("\x1bO{letter}")
        } else {
            format!("\x1b[1;{m}{letter}")
        };
        Some(seq.into_bytes())
    };

    // Insert, Delete, PageUp/Down, F5-F12: a number and `~`.
    // `\x1b[{n}~` without modifiers, `\x1b[{n};{m}~` with them, e.g. Ctrl+Delete: `\x1b[3;5~`.
    let tilde = |n: u8| -> Option<Vec<u8>> {
        let seq = if m == 1 {
            format!("\x1b[{n}~")
        } else {
            format!("\x1b[{n};{m}~")
        };
        Some(seq.into_bytes())
    };

    let bytes: &[u8] = match key {
        Key::Named(NamedKey::Enter) => b"\r", // carriage return, not "\n"
        Key::Named(NamedKey::Backspace) => b"\x7f", // the DEL character
        Key::Named(NamedKey::Tab) => b"\t",
        Key::Named(NamedKey::Escape) => b"\x1b",
        Key::Named(NamedKey::ArrowUp) => return with_mods('A'),
        Key::Named(NamedKey::ArrowDown) => return with_mods('B'),
        Key::Named(NamedKey::ArrowRight) => return with_mods('C'),
        Key::Named(NamedKey::ArrowLeft) => return with_mods('D'),
        Key::Named(NamedKey::Home) => return with_mods('H'),
        Key::Named(NamedKey::End) => return with_mods('F'),
        Key::Named(NamedKey::F1) => return f1_to_f4('P'),
        Key::Named(NamedKey::F2) => return f1_to_f4('Q'),
        Key::Named(NamedKey::F3) => return f1_to_f4('R'),
        Key::Named(NamedKey::F4) => return f1_to_f4('S'),
        Key::Named(NamedKey::Insert) => return tilde(2),
        Key::Named(NamedKey::Delete) => return tilde(3),
        Key::Named(NamedKey::PageUp) => return tilde(5),
        Key::Named(NamedKey::PageDown) => return tilde(6),
        // The gaps (16, 22) are from the VT220 keyboard, where they were
        // spaces between groups of keys. Every terminal kept the numbers.
        Key::Named(NamedKey::F5) => return tilde(15),
        Key::Named(NamedKey::F6) => return tilde(17),
        Key::Named(NamedKey::F7) => return tilde(18),
        Key::Named(NamedKey::F8) => return tilde(19),
        Key::Named(NamedKey::F9) => return tilde(20),
        Key::Named(NamedKey::F10) => return tilde(21),
        Key::Named(NamedKey::F11) => return tilde(23),
        Key::Named(NamedKey::F12) => return tilde(24),
        // Everything else (letters, digits, Cyrillic...): the typed text, as UTF-8 bytes.
        _ => text?.as_bytes(),
    };

    // Alt+key: ESC, then the key, e.g. Alt+f sends `\x1bf`. Not with Ctrl:
    // Ctrl+Alt is AltGr on many layouts, and AltGr types letters like `@` or `ł`.
    if mods.alt_key() && !mods.control_key() {
        return Some([b"\x1b", bytes].concat());
    }
    Some(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: ModifiersState = ModifiersState::CONTROL;
    const CTRL_SHIFT: ModifiersState = ModifiersState::CONTROL.union(ModifiersState::SHIFT);

    fn key(code: KeyCode) -> PhysicalKey {
        PhysicalKey::Code(code)
    }

    /// The bytes a named key sends, as a string, e.g. "\x1b[15~".
    fn send(key: NamedKey, mods: ModifiersState) -> String {
        let bytes = key_to_bytes(&Key::Named(key), None, mods).unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn tab_shortcuts() {
        assert_eq!(
            shortcut(key(KeyCode::KeyT), CTRL_SHIFT),
            Some(Action::NewTab)
        );
        assert_eq!(shortcut(key(KeyCode::Tab), CTRL), Some(Action::NextTab));
        assert_eq!(
            shortcut(key(KeyCode::Tab), CTRL_SHIFT),
            Some(Action::PreviousTab)
        );
        assert_eq!(
            shortcut(key(KeyCode::Digit3), CTRL),
            Some(Action::GoToTab(3))
        );
        assert_eq!(shortcut(key(KeyCode::Digit9), CTRL), Some(Action::LastTab));
    }

    #[test]
    fn ctrl_c_and_ctrl_shift_c() {
        assert_eq!(shortcut(key(KeyCode::KeyC), CTRL), Some(Action::Interrupt));
        assert_eq!(shortcut(key(KeyCode::KeyC), CTRL_SHIFT), Some(Action::Copy));
    }

    #[test]
    fn not_shortcuts() {
        // Without Ctrl, letters go to the shell.
        assert_eq!(shortcut(key(KeyCode::KeyT), ModifiersState::SHIFT), None);
        // Ctrl+T without Shift goes to the shell too (it has its own meaning there).
        assert_eq!(shortcut(key(KeyCode::KeyT), CTRL), None);
        // AltGr (Ctrl+Alt) types letters, it's never a shortcut.
        assert_eq!(
            shortcut(key(KeyCode::KeyC), CTRL | ModifiersState::ALT),
            None
        );
    }

    #[test]
    fn paste() {
        assert_eq!(paste_bytes("a\r\nb\nc", false), b"a\rb\rc");
        assert_eq!(paste_bytes("a\nb", true), b"\x1b[200~a\rb\x1b[201~");
        // A hidden end marker would let `rm -rf ~` run as if typed: it's removed.
        let evil = "echo hi\x1b[201~rm -rf ~\n";
        assert_eq!(
            paste_bytes(evil, true),
            b"\x1b[200~echo hirm -rf ~\r\x1b[201~"
        );
    }

    #[test]
    fn alt_adds_escape() {
        let alt_f = key_to_bytes(&Key::Character("f".into()), Some("f"), ModifiersState::ALT);
        assert_eq!(alt_f.unwrap(), b"\x1bf");
        assert_eq!(send(NamedKey::Backspace, ModifiersState::ALT), "\x1b\x7f");
        // Ctrl+Alt is AltGr: the typed letter goes as it is.
        let altgr = ModifiersState::ALT | ModifiersState::CONTROL;
        assert_eq!(
            key_to_bytes(&Key::Character("@".into()), Some("@"), altgr).unwrap(),
            b"@"
        );
        // Arrows already carry Alt in their number, no extra ESC.
        assert_eq!(send(NamedKey::ArrowUp, ModifiersState::ALT), "\x1b[1;3A");
    }

    #[test]
    fn f1_to_f4() {
        assert_eq!(send(NamedKey::F1, ModifiersState::empty()), "\x1bOP");
        assert_eq!(send(NamedKey::F4, ModifiersState::empty()), "\x1bOS");
        assert_eq!(send(NamedKey::F1, ModifiersState::SHIFT), "\x1b[1;2P");
    }

    #[test]
    fn tilde_keys() {
        assert_eq!(send(NamedKey::Insert, ModifiersState::empty()), "\x1b[2~");
        assert_eq!(send(NamedKey::F5, ModifiersState::empty()), "\x1b[15~");
        assert_eq!(send(NamedKey::F12, ModifiersState::empty()), "\x1b[24~");
        assert_eq!(send(NamedKey::Delete, ModifiersState::CONTROL), "\x1b[3;5~");
    }
}
