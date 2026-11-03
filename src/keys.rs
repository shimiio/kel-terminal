//! Keys: Kel's own shortcuts, and the bytes every other key sends to the shell.
//!
//! When a key is pressed, `App::on_key` asks `shortcut` first. If it's one of
//! Kel's shortcuts, Kel runs the `Action`; if not, `key_to_bytes` turns the key
//! into bytes for the shell. Nothing here knows about windows or tabs, so it
//! can all be tested with plain values.

use crate::palette::Item;
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};

/// Something Kel does: one of its shortcuts, or a line in the command palette.
/// Not `Copy`: `OpenProject` holds a `String`, which is cloned, not copied.
#[derive(Clone, Debug, PartialEq)]
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
    /// Moves the tab on screen one place left / right.
    MoveTabLeft,
    MoveTabRight,
    /// Opens the sidebar of projects, or closes it.
    Sidebar,
    /// Opens the command palette, or closes it.
    Palette,
    /// Opens the help, all the keys and the settings, or closes it.
    Help,
    /// Opens the search in the output, or closes it.
    Find,
    /// Runs the install script of this name (see `setup`) in a new tab.
    Install(&'static str),
    /// Goes to the project of this name in this window (see `project`); the one
    /// on screen goes out of sight, still running.
    OpenProject(String),
    /// Goes home in this window: Kel without a project, `home` in the sidebar;
    /// its start screen, position 0 (in home too). Ctrl+Shift+H.
    OpenHome,
    /// Closes the project on screen: saves it, ends its shells, goes home.
    CloseProject,
    /// Deletes the project on screen: its file, not its folder (see
    /// `project::delete`); then home.
    DeleteProject,
    /// Opens the card of the tab on screen: its name and command.
    EditTab,
    /// Opens the card of the project on screen, in the sidebar: its name and color.
    EditProject,
    /// Saves the tabs as a new project of this name; the window becomes it.
    SaveProject(String),
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
        (KeyCode::PageUp, true) => Action::MoveTabLeft, // like browsers too
        (KeyCode::PageDown, true) => Action::MoveTabRight,
        (KeyCode::KeyV, true) => Action::Paste,
        (KeyCode::KeyC, true) => Action::Copy,
        (KeyCode::KeyC, false) => Action::Interrupt,
        // With or without Shift: Ctrl+Shift+= is Ctrl++.
        (KeyCode::Equal, _) => Action::ZoomIn,
        (KeyCode::Minus, _) => Action::ZoomOut,
        (KeyCode::Digit0, _) => Action::ZoomReset,
        (KeyCode::Comma, _) => Action::OpenConfig,
        (KeyCode::KeyP, true) => Action::Palette,
        (KeyCode::KeyF, true) => Action::Find,
        (KeyCode::KeyB, true) => Action::Sidebar, // like VS Code's Ctrl+B
        (KeyCode::KeyH, true) => Action::OpenHome,
        // Ctrl+? : the question mark's key. Not F1, which programs have (htop,
        // mc, nvim: their own help); not Ctrl+/ alone, which nvim plugins use.
        (KeyCode::Slash, true) => Action::Help,
        _ => return None,
    };
    Some(action)
}

/// Kel's actions as the command palette lists them, with their shortcuts from the
/// table above (a test checks they match).
pub fn palette_items() -> Vec<Item> {
    let item = |label: &str, hint: &str, action| Item {
        label: label.into(),
        hint: hint.into(),
        action,
    };
    vec![
        item("New tab", "Ctrl+Shift+T", Action::NewTab),
        item("Close tab", "Ctrl+Shift+W", Action::CloseTab),
        item("Next tab", "Ctrl+Tab", Action::NextTab),
        item("Previous tab", "Ctrl+Shift+Tab", Action::PreviousTab),
        item("Last tab", "Ctrl+9", Action::LastTab),
        item("Move tab left", "Ctrl+Shift+PageUp", Action::MoveTabLeft),
        item(
            "Move tab right",
            "Ctrl+Shift+PageDown",
            Action::MoveTabRight,
        ),
        item("Paste", "Ctrl+Shift+V", Action::Paste),
        item("Copy the selection", "Ctrl+Shift+C", Action::Copy),
        item("Find in the output", "Ctrl+Shift+F", Action::Find),
        item("Projects: the sidebar", "Ctrl+Shift+B", Action::Sidebar),
        item("Home: its start screen", "Ctrl+Shift+H", Action::OpenHome),
        item("Stop the running command", "Ctrl+C", Action::Interrupt),
        item("Zoom in: bigger font", "Ctrl+=", Action::ZoomIn),
        item("Zoom out: smaller font", "Ctrl+-", Action::ZoomOut),
        item("Reset zoom", "Ctrl+0", Action::ZoomReset),
        item("Open the config file", "Ctrl+,", Action::OpenConfig),
        item("Help: keys and settings", "Ctrl+Shift+/", Action::Help),
    ]
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

/// What pasted text would do in a shell that runs each line as it comes (no
/// bracketed paste): the text to paste, and how many lines it holds if they'd
/// run on their own, 0 if none would.
/// - One line, with a line break after it (copied with its end, as a site's
///   code block gives it): just the line. It waits for your Enter.
/// - Several lines: as they are, and their number: worth asking first. A site
///   can show one line and copy two, the second hidden.
pub fn plain_paste(text: &str) -> (&str, usize) {
    let body = text.trim_end_matches(['\r', '\n']);
    if body.contains(['\r', '\n']) {
        (text, body.lines().count())
    } else {
        (body, 0)
    }
}

/// Translates a key press into the bytes a terminal would send to the shell.
/// `key` is what the layout says the key is (without Ctrl: `d` for Ctrl+D), and where
/// it is on the keyboard. `None` means the key sends nothing (e.g. Shift alone).
pub fn key_to_bytes(
    (key, physical): (&Key, PhysicalKey),
    text: Option<&str>,
    mods: ModifiersState,
) -> Option<Vec<u8>> {
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

    // Ctrl, but not AltGr (Ctrl+Alt), which types letters.
    let ctrl = mods.control_key() && !mods.alt_key();

    let bytes: &[u8] = match key {
        Key::Named(NamedKey::Enter) => b"\r", // carriage return, not "\n"
        // Ctrl+Backspace: Ctrl+H, as in Windows Terminal; PowerShell deletes a word.
        Key::Named(NamedKey::Backspace) if ctrl => b"\x08",
        Key::Named(NamedKey::Backspace) => b"\x7f", // the DEL character
        // Shift+Tab: "back tab", e.g. the previous item in a completion list.
        Key::Named(NamedKey::Tab) if mods.shift_key() => b"\x1b[Z",
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
        // Ctrl+D, Ctrl+[ ...: one control character.
        _ if ctrl && control_char(key, physical).is_some() => {
            return control_char(key, physical).map(|c| vec![c]);
        }
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

/// The control character Ctrl+`key` sends. Terminals have no "Ctrl is down": Ctrl
/// keeps only the low 5 bits of the character, so Ctrl+D (`D` = 0x44) is 0x04, and
/// Ctrl+[ (`[` = 0x5B) is 0x1B, the same byte as Esc.
///
/// - By what the layout says the key is: Ctrl+Z is the key with Z on it, also where
///   German keyboards have it (where US ones have Y).
/// - A letter that isn't Latin (Ctrl+В on a Russian layout): by the key's place, D.
///   Only on the A–Z keys: German Ü is where US keyboards have `[`, and Ctrl+Ü
///   mustn't be Esc; French AZERTY has a comma on M, and Ctrl+, mustn't be Enter.
/// - `None` for everything else (Ctrl+ü, Ctrl+1): no control character.
fn control_char(key: &Key, physical: PhysicalKey) -> Option<u8> {
    let Key::Character(text) = key else {
        return None;
    };
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    match c {
        'a'..='z' | 'A'..='Z' | '[' | '\\' | ']' | '^' | '_' => Some(c as u8 & 0x1F),
        '/' => Some(0x1F), // like Ctrl+_, as in xterm
        c if c.is_alphabetic() && !c.is_ascii() => letter_key(physical).map(|c| c & 0x1F),
        _ => None,
    }
}

/// The Latin letter on this key of a US keyboard, if it's one of A–Z.
fn letter_key(physical: PhysicalKey) -> Option<u8> {
    const LETTERS: [KeyCode; 26] = [
        KeyCode::KeyA,
        KeyCode::KeyB,
        KeyCode::KeyC,
        KeyCode::KeyD,
        KeyCode::KeyE,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyI,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::KeyL,
        KeyCode::KeyM,
        KeyCode::KeyN,
        KeyCode::KeyO,
        KeyCode::KeyP,
        KeyCode::KeyQ,
        KeyCode::KeyR,
        KeyCode::KeyS,
        KeyCode::KeyT,
        KeyCode::KeyU,
        KeyCode::KeyV,
        KeyCode::KeyW,
        KeyCode::KeyX,
        KeyCode::KeyY,
        KeyCode::KeyZ,
    ];
    let PhysicalKey::Code(code) = physical else {
        return None;
    };
    let i = LETTERS.iter().position(|&letter| letter == code)?;
    Some(b'a' + i as u8)
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
        let place = PhysicalKey::Code(KeyCode::F35); // named keys don't look at it
        let bytes = key_to_bytes((&Key::Named(key), place), None, mods).unwrap();
        String::from_utf8(bytes).unwrap()
    }

    /// The bytes the key at `place` sends when the layout calls it `text`
    /// (winit leaves Ctrl out of it: `d` for Ctrl+D).
    fn type_key(text: &str, place: KeyCode, mods: ModifiersState) -> Vec<u8> {
        let key = Key::Character(text.into());
        key_to_bytes((&key, PhysicalKey::Code(place)), Some(text), mods).unwrap()
    }

    #[test]
    fn ctrl_letters_are_control_characters() {
        assert_eq!(type_key("d", KeyCode::KeyD, CTRL), b"\x04");
        assert_eq!(type_key("u", KeyCode::KeyU, CTRL), b"\x15");
        assert_eq!(type_key("a", KeyCode::KeyA, CTRL), b"\x01");
        assert_eq!(type_key("z", KeyCode::KeyZ, CTRL), b"\x1a");
        // With Shift the layout says `D`: the same character.
        assert_eq!(type_key("D", KeyCode::KeyD, CTRL_SHIFT), b"\x04");
        // Without Ctrl, still the letter.
        assert_eq!(type_key("d", KeyCode::KeyD, ModifiersState::empty()), b"d");
    }

    #[test]
    fn ctrl_punctuation() {
        assert_eq!(type_key("[", KeyCode::BracketLeft, CTRL), b"\x1b");
        assert_eq!(type_key("\\", KeyCode::Backslash, CTRL), b"\x1c");
        assert_eq!(type_key("]", KeyCode::BracketRight, CTRL), b"\x1d");
        assert_eq!(type_key("^", KeyCode::Digit6, CTRL_SHIFT), b"\x1e");
        assert_eq!(type_key("/", KeyCode::Slash, CTRL), b"\x1f");
    }

    #[test]
    fn ctrl_on_other_layouts() {
        // Russian: Ctrl+В is the D key.
        assert_eq!(type_key("в", KeyCode::KeyD, CTRL), b"\x04");
        // German: Z is where US keyboards have Y; it goes by the letter on it.
        assert_eq!(type_key("z", KeyCode::KeyY, CTRL), b"\x1a");
        // German Ü is where US keyboards have `[`: not Esc.
        assert_eq!(type_key("ü", KeyCode::BracketLeft, CTRL), "ü".as_bytes());
        // French AZERTY: a comma on M is not Ctrl+M (Enter).
        assert_eq!(type_key(",", KeyCode::KeyM, CTRL), b",");
        // AltGr (Ctrl+Alt) types letters, e.g. Polish ć: not a control character.
        let altgr = CTRL | ModifiersState::ALT;
        assert_eq!(type_key("ć", KeyCode::KeyC, altgr), "ć".as_bytes());
    }

    #[test]
    fn shift_tab_and_ctrl_backspace() {
        assert_eq!(send(NamedKey::Tab, ModifiersState::SHIFT), "\x1b[Z");
        assert_eq!(send(NamedKey::Tab, ModifiersState::empty()), "\t");
        assert_eq!(send(NamedKey::Backspace, CTRL), "\x08");
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
    fn home_is_ctrl_shift_h() {
        let home = shortcut(key(KeyCode::KeyH), CTRL_SHIFT);
        assert_eq!(home, Some(Action::OpenHome));
        // Ctrl+H alone is the shell's (a Backspace, in some programs).
        assert_eq!(shortcut(key(KeyCode::KeyH), CTRL), None);
    }

    #[test]
    fn ctrl_c_and_ctrl_shift_c() {
        assert_eq!(shortcut(key(KeyCode::KeyC), CTRL), Some(Action::Interrupt));
        assert_eq!(shortcut(key(KeyCode::KeyC), CTRL_SHIFT), Some(Action::Copy));
    }

    /// "Ctrl+Shift+T" → the T key with Ctrl and Shift held.
    fn parse_hint(hint: &str) -> (PhysicalKey, ModifiersState) {
        let mut mods = ModifiersState::empty();
        let mut parts: Vec<&str> = hint.split('+').collect();
        // Ctrl+- and Ctrl+= : the last part is the key, even when it's a `+`-less sign.
        let last = parts.pop().unwrap();
        for part in parts {
            match part {
                "Ctrl" => mods |= ModifiersState::CONTROL,
                "Shift" => mods |= ModifiersState::SHIFT,
                _ => panic!("unknown modifier in {hint}"),
            }
        }
        let code = match last {
            "Tab" => KeyCode::Tab,
            "PageUp" => KeyCode::PageUp,
            "PageDown" => KeyCode::PageDown,
            "=" => KeyCode::Equal,
            "-" => KeyCode::Minus,
            "," => KeyCode::Comma,
            "/" => KeyCode::Slash,
            "0" => KeyCode::Digit0,
            "9" => KeyCode::Digit9,
            "B" => KeyCode::KeyB,
            "C" => KeyCode::KeyC,
            "F" => KeyCode::KeyF,
            "H" => KeyCode::KeyH,
            "T" => KeyCode::KeyT,
            "V" => KeyCode::KeyV,
            "W" => KeyCode::KeyW,
            _ => panic!("add {last} to parse_hint"),
        };
        (PhysicalKey::Code(code), mods)
    }

    #[test]
    fn palette_hints_are_the_real_shortcuts() {
        for item in palette_items() {
            let (key, mods) = parse_hint(&item.hint);
            assert_eq!(shortcut(key, mods), Some(item.action), "{}", item.label);
        }
    }

    #[test]
    fn palette_shortcut() {
        assert_eq!(
            shortcut(key(KeyCode::KeyP), CTRL_SHIFT),
            Some(Action::Palette)
        );
        // Ctrl+P alone goes to the program (nvim: up in a list).
        assert_eq!(shortcut(key(KeyCode::KeyP), CTRL), None);
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
    fn several_lines_are_worth_asking() {
        assert_eq!(plain_paste("ls"), ("ls", 0));
        // The line break after one line goes: it would run it.
        assert_eq!(plain_paste("ls -la\r\n"), ("ls -la", 0));
        assert_eq!(plain_paste("ls\n\n"), ("ls", 0));
        // Two lines: the first would run at once.
        let hidden = "echo \"harmless\"\necho \"HIDDEN COMMAND RAN\"";
        assert_eq!(plain_paste(hidden), (hidden, 2));
        assert_eq!(plain_paste("a\r\nb\r\nc\r\n"), ("a\r\nb\r\nc\r\n", 3));
    }

    #[test]
    fn alt_adds_escape() {
        assert_eq!(type_key("f", KeyCode::KeyF, ModifiersState::ALT), b"\x1bf");
        assert_eq!(send(NamedKey::Backspace, ModifiersState::ALT), "\x1b\x7f");
        // Ctrl+Alt is AltGr: the typed letter goes as it is.
        let altgr = ModifiersState::ALT | ModifiersState::CONTROL;
        assert_eq!(type_key("@", KeyCode::KeyQ, altgr), b"@");
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
