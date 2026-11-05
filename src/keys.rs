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
    /// Copies the mouse selection; without one, acts as `Interrupt` (not on a
    /// Mac: Cmd+C there only copies, as in its other apps).
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
    /// The next open project in this window, in the sidebar's order, home first
    /// and always there; after the last one, home again. Ctrl+Alt+→, like Zen's
    /// workspaces.
    NextProject,
    /// The previous one, the other way round. Ctrl+Alt+←.
    PreviousProject,
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
    /// Cmd+Q on a Mac: as the window's close button (it asks first if a
    /// command runs).
    Quit,
    /// Cmd+H on a Mac: hides Kel, as every Mac app.
    Hide,
    /// Cmd+M on a Mac: the window to the Dock.
    Minimize,
}

/// Kel runs on a Mac: Cmd for its shortcuts, not Ctrl, and Option for letters.
pub const MAC: bool = cfg!(target_os = "macos");

/// Kel's key, as hints write it: Cmd on a Mac, Ctrl elsewhere.
pub const MOD: &str = if MAC { "Cmd" } else { "Ctrl" };

/// Alt, as hints write it: a Mac calls it Option.
pub const ALT: &str = if MAC { "Option" } else { "Alt" };

/// Is Kel's key held (Ctrl+click on a link, Ctrl+V in a card): Cmd on a Mac,
/// Ctrl elsewhere.
pub fn command_key(mods: ModifiersState) -> bool {
    if MAC {
        mods.super_key()
    } else {
        mods.control_key()
    }
}

/// Which of Kel's shortcuts this key is, if any. All of Kel's shortcuts, in one table.
///
/// Uses the physical key (its place on the keyboard), so shortcuts work with
/// any layout, Cyrillic too.
pub fn shortcut(key: PhysicalKey, mods: ModifiersState) -> Option<Action> {
    shortcut_on(key, mods, MAC)
}

/// `shortcut` on a Mac (`mac`) or not: both are tested everywhere.
fn shortcut_on(key: PhysicalKey, mods: ModifiersState, mac: bool) -> Option<Action> {
    let PhysicalKey::Code(code) = key else {
        return None;
    };
    if mac {
        mac_shortcut(code, mods)
    } else {
        pc_shortcut(code, mods)
    }
}

/// Tab 1…8 by its digit key.
fn tab_number(code: KeyCode) -> Option<usize> {
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
    ];
    digits
        .iter()
        .position(|&digit| digit == code)
        .map(|i| i + 1)
}

/// Kel's shortcuts on Windows and Linux.
fn pc_shortcut(code: KeyCode, mods: ModifiersState) -> Option<Action> {
    // Every shortcut uses Ctrl. Not with Alt: Ctrl+Alt is AltGr on many layouts,
    // and AltGr types letters (Polish AltGr+C is `ć`, not a Ctrl+C). Except the
    // arrows: they type nothing, with AltGr neither.
    if mods.control_key() && mods.alt_key() && !mods.shift_key() {
        return match code {
            KeyCode::ArrowRight => Some(Action::NextProject),
            KeyCode::ArrowLeft => Some(Action::PreviousProject),
            _ => None,
        };
    }
    if !mods.control_key() || mods.alt_key() {
        return None;
    }
    let action = match (code, mods.shift_key()) {
        // (key, with Shift?)
        (KeyCode::KeyT, true) => Action::NewTab,
        (KeyCode::KeyW, true) => Action::CloseTab,
        (KeyCode::Tab, false) => Action::NextTab,
        (KeyCode::Tab, true) => Action::PreviousTab,
        (code, false) if tab_number(code).is_some() => Action::GoToTab(tab_number(code)?),
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

/// Kel's shortcuts on a Mac: with Cmd, as its other apps have them. Ctrl is
/// the shell's there, as in Terminal (Ctrl+C, Ctrl+R, Ctrl+T), so Cmd needs no
/// Shift to stay out of its way; but Ctrl+Tab, which Terminal has too. Cmd+Q,
/// Cmd+H and Cmd+M are Kel's own: the Mac's menu would quit without saving.
fn mac_shortcut(code: KeyCode, mods: ModifiersState) -> Option<Action> {
    let (cmd, ctrl, alt, shift) = (
        mods.super_key(),
        mods.control_key(),
        mods.alt_key(),
        mods.shift_key(),
    );
    if ctrl && !cmd && !alt {
        return match (code, shift) {
            (KeyCode::Tab, false) => Some(Action::NextTab),
            (KeyCode::Tab, true) => Some(Action::PreviousTab),
            (KeyCode::KeyC, false) => Some(Action::Interrupt),
            _ => None,
        };
    }
    if !cmd || ctrl {
        return None;
    }
    // Cmd+Option: the arrows only. Option types letters on a Mac.
    if alt {
        return match (code, shift) {
            (KeyCode::ArrowRight, false) => Some(Action::NextProject),
            (KeyCode::ArrowLeft, false) => Some(Action::PreviousProject),
            _ => None,
        };
    }
    let action = match (code, shift) {
        // (key, with Shift?)
        (KeyCode::KeyT, false) => Action::NewTab,
        (KeyCode::KeyW, false) => Action::CloseTab,
        // Cmd+} and Cmd+{, as Safari and Terminal.
        (KeyCode::BracketRight, true) => Action::NextTab,
        (KeyCode::BracketLeft, true) => Action::PreviousTab,
        (code, false) if tab_number(code).is_some() => Action::GoToTab(tab_number(code)?),
        (KeyCode::Digit9, false) => Action::LastTab,
        // A Mac's keyboard has no PageUp: the arrows, as iTerm.
        (KeyCode::ArrowLeft, true) => Action::MoveTabLeft,
        (KeyCode::ArrowRight, true) => Action::MoveTabRight,
        (KeyCode::KeyV, false) => Action::Paste,
        (KeyCode::KeyC, false) => Action::Copy,
        (KeyCode::Equal, _) => Action::ZoomIn,
        (KeyCode::Minus, _) => Action::ZoomOut,
        (KeyCode::Digit0, _) => Action::ZoomReset,
        (KeyCode::Comma, false) => Action::OpenConfig,
        (KeyCode::KeyP, true) => Action::Palette,
        (KeyCode::KeyF, false) => Action::Find,
        (KeyCode::KeyB, false) => Action::Sidebar,
        // Cmd+H alone hides, on every Mac.
        (KeyCode::KeyH, true) => Action::OpenHome,
        // Cmd+?, as the Help menu of Mac apps.
        (KeyCode::Slash, true) => Action::Help,
        (KeyCode::KeyQ, false) => Action::Quit,
        (KeyCode::KeyH, false) => Action::Hide,
        (KeyCode::KeyM, false) => Action::Minimize,
        _ => return None,
    };
    Some(action)
}

/// Kel's actions as the command palette lists them, with their shortcuts from the
/// table above (a test checks they match).
pub fn palette_items() -> Vec<Item> {
    palette_items_on(MAC)
}

/// The shortcut of `action`, as the palette shows it ("Ctrl+Shift+T"); empty
/// for one without. The palette's own isn't in its list: it's open by then.
pub fn hint(action: &Action) -> String {
    if *action == Action::Palette {
        return if MAC { "Cmd+Shift+P" } else { "Ctrl+Shift+P" }.into();
    }
    palette_items()
        .into_iter()
        .find(|item| item.action == *action)
        .map(|item| item.hint)
        .unwrap_or_default()
}

/// `palette_items` on a Mac (`mac`) or not: each has its keys.
fn palette_items_on(mac: bool) -> Vec<Item> {
    let item = |label: &str, pc: &str, on_mac: &str, action| Item {
        label: label.into(),
        hint: if mac { on_mac } else { pc }.into(),
        action,
    };
    let mut items = vec![
        item("New tab", "Ctrl+Shift+T", "Cmd+T", Action::NewTab),
        item("Close tab", "Ctrl+Shift+W", "Cmd+W", Action::CloseTab),
        item("Next tab", "Ctrl+Tab", "Cmd+Shift+]", Action::NextTab),
        item(
            "Previous tab",
            "Ctrl+Shift+Tab",
            "Cmd+Shift+[",
            Action::PreviousTab,
        ),
        item("Last tab", "Ctrl+9", "Cmd+9", Action::LastTab),
        item(
            "Move tab left",
            "Ctrl+Shift+PageUp",
            "Cmd+Shift+←",
            Action::MoveTabLeft,
        ),
        item(
            "Move tab right",
            "Ctrl+Shift+PageDown",
            "Cmd+Shift+→",
            Action::MoveTabRight,
        ),
        item("Paste", "Ctrl+Shift+V", "Cmd+V", Action::Paste),
        item("Copy the selection", "Ctrl+Shift+C", "Cmd+C", Action::Copy),
        item("Find in the output", "Ctrl+Shift+F", "Cmd+F", Action::Find),
        item(
            "Projects: the sidebar",
            "Ctrl+Shift+B",
            "Cmd+B",
            Action::Sidebar,
        ),
        item(
            "Home: its start screen",
            "Ctrl+Shift+H",
            "Cmd+Shift+H",
            Action::OpenHome,
        ),
        item(
            "Next open project",
            "Ctrl+Alt+→",
            "Cmd+Option+→",
            Action::NextProject,
        ),
        item(
            "Previous open project",
            "Ctrl+Alt+←",
            "Cmd+Option+←",
            Action::PreviousProject,
        ),
        item(
            "Stop the running command",
            "Ctrl+C",
            "Ctrl+C",
            Action::Interrupt,
        ),
        item("Zoom in: bigger font", "Ctrl+=", "Cmd+=", Action::ZoomIn),
        item("Zoom out: smaller font", "Ctrl+-", "Cmd+-", Action::ZoomOut),
        item("Reset zoom", "Ctrl+0", "Cmd+0", Action::ZoomReset),
        item(
            "Open the config file",
            "Ctrl+,",
            "Cmd+,",
            Action::OpenConfig,
        ),
        item(
            "Help: keys and settings",
            "Ctrl+Shift+/",
            "Cmd+Shift+/",
            Action::Help,
        ),
    ];
    // A Mac's own, which Windows and Linux leave to the window's buttons.
    if mac {
        items.extend([
            item("Minimize the window", "", "Cmd+M", Action::Minimize),
            item("Hide Kel", "", "Cmd+H", Action::Hide),
            item("Quit Kel", "", "Cmd+Q", Action::Quit),
        ]);
    }
    items
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
        // Alt+М on a Russian layout is Alt+V, as Ctrl+В is Ctrl+D: programs bind
        // Alt to Latin letters (Claude Code's Alt+V pastes an image, PowerShell's
        // Alt+D deletes a word), none to Cyrillic ones.
        if let Some(latin) = latin_letter(key, physical) {
            return Some(vec![0x1b, latin]);
        }
        return Some([b"\x1b", bytes].concat());
    }
    Some(bytes.to_vec())
}

/// `key_to_bytes` on a Mac, where Cmd and Option are their own:
/// - Cmd+key is Kel's, or nothing; but Cmd+← and Cmd+→ go to the line's start
///   and end, and Cmd+Backspace deletes it (Ctrl+A, Ctrl+E, Ctrl+U), as in
///   Terminal and Ghostty.
/// - Option+← and Option+→: a word back, on (`ESC b`, `ESC f`), the same.
/// - Option+letter types what the Mac makes of it (`@` and `{` on German and
///   other layouts, which have them only so), unless `option_as_alt`: then
///   it's Alt, `ESC` and the letter, for Alt+B in a shell or nvim's Alt keys.
///   With it on, Kel asks the Mac for the letter without Option (see `App`).
pub fn mac_key_to_bytes(
    (key, physical): (&Key, PhysicalKey),
    text: Option<&str>,
    mods: ModifiersState,
    option_as_alt: bool,
) -> Option<Vec<u8>> {
    if mods.super_key() {
        let bytes: &[u8] = match key {
            Key::Named(NamedKey::ArrowLeft) => b"\x01",
            Key::Named(NamedKey::ArrowRight) => b"\x05",
            Key::Named(NamedKey::Backspace) => b"\x15",
            _ => return None,
        };
        return Some(bytes.to_vec());
    }
    if mods == ModifiersState::ALT {
        match key {
            Key::Named(NamedKey::ArrowLeft) => return Some(b"\x1bb".to_vec()),
            Key::Named(NamedKey::ArrowRight) => return Some(b"\x1bf".to_vec()),
            _ => {}
        }
    }
    let mut mods = mods;
    if !option_as_alt && matches!(key, Key::Character(_)) {
        mods.remove(ModifiersState::ALT);
    }
    key_to_bytes((key, physical), text, mods)
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

/// The arrows, Home and End the way a program asked for them with `ESC[?1h`
/// ("application cursor keys"): `ESC O A`, not `ESC [ A`. Only without Ctrl,
/// Shift or Alt: with them it's `ESC[1;5A` either way. less, htop and other
/// programs that look the keys up in terminfo know them only so.
pub fn app_cursor(bytes: Vec<u8>) -> Vec<u8> {
    match bytes.as_slice() {
        [
            0x1b,
            b'[',
            letter @ (b'A' | b'B' | b'C' | b'D' | b'H' | b'F'),
        ] => {
            vec![0x1b, b'O', *letter]
        }
        _ => bytes,
    }
}

/// For a letter that isn't Latin (`м` on a Russian layout), the Latin one on its
/// key (`v`), in the same case (`М`: `V`). `None` for anything else: a Latin
/// letter is itself, and German `ü` (on US `[`) has no letter's place.
fn latin_letter(key: &Key, physical: PhysicalKey) -> Option<u8> {
    let Key::Character(text) = key else {
        return None;
    };
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if !c.is_alphabetic() || c.is_ascii() {
        return None;
    }
    let latin = letter_key(physical)?;
    Some(if c.is_uppercase() {
        latin.to_ascii_uppercase()
    } else {
        latin
    })
}

/// Files copied in Explorer (Ctrl+C on them), as text to paste: their paths,
/// a space between them, one with a space in it in `""`. So Ctrl+Shift+V on
/// a copied picture gives `"C:\Shots\my shot.png"`, as Windows Terminal does,
/// and Claude Code attaches the picture at that path.
pub fn paths_text(paths: &[std::path::PathBuf]) -> String {
    let quoted: Vec<String> = paths
        .iter()
        .map(|path| {
            let path = path.display().to_string();
            if path.contains(' ') {
                format!("\"{path}\"")
            } else {
                path
            }
        })
        .collect();
    quoted.join(" ")
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

    /// Windows' and Linux's shortcuts, on a Mac too (`mac_shortcuts` has its own).
    fn shortcut(key: PhysicalKey, mods: ModifiersState) -> Option<Action> {
        shortcut_on(key, mods, false)
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
    fn ctrl_alt_arrows_go_through_projects() {
        let ctrl_alt = ModifiersState::CONTROL | ModifiersState::ALT;
        let right = shortcut(key(KeyCode::ArrowRight), ctrl_alt);
        assert_eq!(right, Some(Action::NextProject));
        let left = shortcut(key(KeyCode::ArrowLeft), ctrl_alt);
        assert_eq!(left, Some(Action::PreviousProject));
        // Ctrl+Alt with a letter is AltGr typing it: not Kel's.
        assert_eq!(shortcut(key(KeyCode::KeyC), ctrl_alt), None);
        // Ctrl+→ alone is the shell's (a word to the right).
        assert_eq!(shortcut(key(KeyCode::ArrowRight), CTRL), None);
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
                "Alt" | "Option" => mods |= ModifiersState::ALT,
                "Cmd" => mods |= ModifiersState::SUPER,
                _ => panic!("unknown modifier in {hint}"),
            }
        }
        let code = match last {
            "Tab" => KeyCode::Tab,
            "→" => KeyCode::ArrowRight,
            "←" => KeyCode::ArrowLeft,
            "PageUp" => KeyCode::PageUp,
            "PageDown" => KeyCode::PageDown,
            "=" => KeyCode::Equal,
            "-" => KeyCode::Minus,
            "," => KeyCode::Comma,
            "/" => KeyCode::Slash,
            "0" => KeyCode::Digit0,
            "9" => KeyCode::Digit9,
            "[" => KeyCode::BracketLeft,
            "]" => KeyCode::BracketRight,
            "B" => KeyCode::KeyB,
            "C" => KeyCode::KeyC,
            "F" => KeyCode::KeyF,
            "H" => KeyCode::KeyH,
            "M" => KeyCode::KeyM,
            "P" => KeyCode::KeyP,
            "Q" => KeyCode::KeyQ,
            "T" => KeyCode::KeyT,
            "V" => KeyCode::KeyV,
            "W" => KeyCode::KeyW,
            _ => panic!("add {last} to parse_hint"),
        };
        (PhysicalKey::Code(code), mods)
    }

    #[test]
    fn palette_hints_are_the_real_shortcuts() {
        for mac in [false, true] {
            for item in palette_items_on(mac) {
                let (key, mods) = parse_hint(&item.hint);
                let found = shortcut_on(key, mods, mac);
                assert_eq!(found, Some(item.action), "{} (mac: {mac})", item.label);
            }
        }
    }

    /// A Mac's keys, as Mac apps have them; Ctrl is the shell's.
    #[test]
    fn mac_shortcuts() {
        let cmd = ModifiersState::SUPER;
        let cmd_shift = cmd | ModifiersState::SHIFT;
        let mac = |code, mods| shortcut_on(key(code), mods, true);
        assert_eq!(mac(KeyCode::KeyT, cmd), Some(Action::NewTab));
        assert_eq!(mac(KeyCode::Digit3, cmd), Some(Action::GoToTab(3)));
        assert_eq!(mac(KeyCode::KeyQ, cmd), Some(Action::Quit));
        assert_eq!(mac(KeyCode::KeyH, cmd), Some(Action::Hide));
        assert_eq!(mac(KeyCode::KeyH, cmd_shift), Some(Action::OpenHome));
        assert_eq!(mac(KeyCode::Tab, CTRL), Some(Action::NextTab));
        // Ctrl+T, Ctrl+Shift+T, Ctrl+R: the shell's on a Mac.
        assert_eq!(mac(KeyCode::KeyT, CTRL), None);
        assert_eq!(mac(KeyCode::KeyT, CTRL_SHIFT), None);
        assert_eq!(mac(KeyCode::KeyR, CTRL), None);
        // Cmd+K: nothing of Kel's (and nothing typed, see `mac_typing`).
        assert_eq!(mac(KeyCode::KeyK, cmd), None);
        // And Cmd isn't Kel's key on Windows and Linux.
        assert_eq!(shortcut_on(key(KeyCode::KeyT), cmd, false), None);
    }

    /// `mac_key_to_bytes` for a key the layout calls `text`.
    fn mac_type(text: &str, place: KeyCode, mods: ModifiersState, alt: bool) -> Option<Vec<u8>> {
        let key = Key::Character(text.into());
        mac_key_to_bytes((&key, PhysicalKey::Code(place)), Some(text), mods, alt)
    }

    #[test]
    fn mac_typing() {
        let option = ModifiersState::ALT;
        // Option types what the Mac makes of it: German Option+L is `@`.
        assert_eq!(
            mac_type("@", KeyCode::KeyL, option, false),
            Some(b"@".to_vec())
        );
        // With `option_as_alt`, it's Alt (the Mac gives the plain letter then).
        assert_eq!(
            mac_type("b", KeyCode::KeyB, option, true),
            Some(b"\x1bb".to_vec())
        );
        // Cmd+letter types nothing.
        assert_eq!(
            mac_type("k", KeyCode::KeyK, ModifiersState::SUPER, false),
            None
        );
        // Ctrl is Ctrl.
        assert_eq!(mac_type("r", KeyCode::KeyR, CTRL, false), Some(vec![0x12]));
        let named = |key: NamedKey, mods| {
            let place = PhysicalKey::Code(KeyCode::F35);
            mac_key_to_bytes((&Key::Named(key), place), None, mods, false).unwrap()
        };
        // Words and lines, as in Terminal.
        assert_eq!(named(NamedKey::ArrowLeft, option), b"\x1bb");
        assert_eq!(named(NamedKey::ArrowRight, option), b"\x1bf");
        assert_eq!(named(NamedKey::ArrowLeft, ModifiersState::SUPER), b"\x01");
        assert_eq!(named(NamedKey::Backspace, ModifiersState::SUPER), b"\x15");
        // Option+Backspace: a word, as Alt+Backspace.
        assert_eq!(named(NamedKey::Backspace, option), b"\x1b\x7f");
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
    fn the_palette_has_a_hint_though_it_doesn_t_list_itself() {
        let hint = hint(&Action::Palette);
        assert!(hint.ends_with("+Shift+P"), "{hint}");
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
    fn alt_on_other_layouts() {
        let alt = ModifiersState::ALT;
        // Russian: Alt+М is the V key, Alt+Shift+М a capital V.
        assert_eq!(type_key("м", KeyCode::KeyV, alt), b"\x1bv");
        assert_eq!(
            type_key("М", KeyCode::KeyV, alt | ModifiersState::SHIFT),
            b"\x1bV"
        );
        // German: Z where US keyboards have Y goes by the letter on it.
        assert_eq!(type_key("z", KeyCode::KeyY, alt), b"\x1bz");
        // German Ü is where US keyboards have `[`: no letter there, Ü it is.
        assert_eq!(type_key("ü", KeyCode::BracketLeft, alt), "\x1bü".as_bytes());
    }

    #[test]
    fn application_cursor_keys() {
        let up = send(NamedKey::ArrowUp, ModifiersState::empty());
        assert_eq!(app_cursor(up.into_bytes()), b"\x1bOA");
        let home = send(NamedKey::Home, ModifiersState::empty());
        assert_eq!(app_cursor(home.into_bytes()), b"\x1bOH");
        // With Ctrl: as it was.
        let ctrl_up = send(NamedKey::ArrowUp, CTRL);
        assert_eq!(app_cursor(ctrl_up.into_bytes()), b"\x1b[1;5A");
        // Not an arrow: as it was.
        assert_eq!(app_cursor(b"\x1b[3~".to_vec()), b"\x1b[3~");
    }

    #[test]
    fn copied_files_paste_as_paths() {
        use std::path::PathBuf;
        let one = [PathBuf::from(r"C:\shots\a.png")];
        assert_eq!(paths_text(&one), r"C:\shots\a.png");
        let two = [
            PathBuf::from(r"C:\my shots\a.png"),
            PathBuf::from(r"C:\b.png"),
        ];
        assert_eq!(paths_text(&two), r#""C:\my shots\a.png" C:\b.png"#);
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
