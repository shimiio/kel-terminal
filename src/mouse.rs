//! Mouse reports: a program (nvim, htop, lazygit) asks for the mouse, and Kel sends
//! it what the mouse does, as bytes, like keys.
//!
//! The program turns it on with a mode (`ESC[?1002h`), see `Tracking`. Each event
//! then goes out in one of two ways:
//!
//! | | Left press at col 4, row 3 | Release |
//! |---|---|---|
//! | SGR (`ESC[?1006h`), what nvim asks for | `ESC[<0;4;3M` | `ESC[<0;4;3m`: small `m` |
//! | The old way | `ESC[M` and 3 bytes: 32 + 0, 32 + 4, 32 + 3 | button 3, "some button" |
//!
//! The old way fits a number in one byte, so it can't say col 224 or further.
//!
//! The first number is the button and more: 0 left, 1 middle, 2 right, 64/65 the
//! wheel up/down; +32 if the mouse moved; +8 with Alt, +16 with Ctrl. Shift isn't
//! there: Shift + mouse stays Kel's, to select text (see `App::program_takes_mouse`).
//!
//! Nothing here knows about windows, so it can all be tested with plain values.

use winit::keyboard::ModifiersState;

/// What the program wants to hear about. Each one adds to the one before.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub enum Tracking {
    /// The mouse is Kel's: it selects text, the wheel scrolls back.
    #[default]
    Off,
    /// `ESC[?1000h`: presses and releases, and the wheel.
    Clicks,
    /// `ESC[?1002h`: also moves while a button is held (dragging). nvim's.
    Drags,
    /// `ESC[?1003h`: every move.
    Moves,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Button {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Press(Button),
    Release(Button),
    /// The mouse moved to another cell, with this button held (or none).
    Move(Option<Button>),
}

/// The bytes that tell the program about `event` at `cell` (row, col, from 0), or
/// `None` if it didn't ask for this kind, or the old way can't say where.
pub fn report(
    tracking: Tracking,
    sgr: bool,
    event: Event,
    modifiers: ModifiersState,
    (row, col): (usize, usize),
) -> Option<Vec<u8>> {
    let (mut code, release) = match event {
        _ if tracking == Tracking::Off => return None,
        Event::Press(button) => (button_code(button), false),
        Event::Release(button) => (button_code(button), true),
        Event::Move(Some(button)) if tracking >= Tracking::Drags => {
            (button_code(button) + 32, false)
        }
        Event::Move(None) if tracking == Tracking::Moves => (3 + 32, false), // 3: no button
        Event::Move(_) => return None,
    };
    if modifiers.alt_key() {
        code += 8;
    }
    if modifiers.control_key() {
        code += 16;
    }
    // Counted from 1 in the report.
    let (x, y) = (col + 1, row + 1);
    if sgr {
        let end = if release { 'm' } else { 'M' };
        return Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes());
    }
    // The old way can't say which button was released: 3 means "the button".
    if release {
        code = code & !0b11 | 3;
    }
    let byte = |n: usize| u8::try_from(32 + n).ok();
    Some(vec![0x1b, b'[', b'M', byte(code)?, byte(x)?, byte(y)?])
}

fn button_code(button: Button) -> usize {
    match button {
        Button::Left => 0,
        Button::Middle => 1,
        Button::Right => 2,
        Button::WheelUp => 64,
        Button::WheelDown => 65,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sgr(tracking: Tracking, event: Event) -> Option<String> {
        report(tracking, true, event, ModifiersState::empty(), (2, 3))
            .map(|bytes| String::from_utf8(bytes).unwrap())
    }

    #[test]
    fn sgr_reports() {
        let drags = Tracking::Drags;
        let left = Button::Left;
        assert_eq!(sgr(drags, Event::Press(left)).unwrap(), "\x1b[<0;4;3M");
        assert_eq!(sgr(drags, Event::Release(left)).unwrap(), "\x1b[<0;4;3m");
        assert_eq!(
            sgr(drags, Event::Press(Button::Right)).unwrap(),
            "\x1b[<2;4;3M"
        );
        assert_eq!(
            sgr(drags, Event::Press(Button::WheelDown)).unwrap(),
            "\x1b[<65;4;3M"
        );
        assert_eq!(
            sgr(drags, Event::Move(Some(left))).unwrap(),
            "\x1b[<32;4;3M"
        );
    }

    #[test]
    fn only_what_the_program_asked_for() {
        let click = Event::Press(Button::Left);
        assert_eq!(sgr(Tracking::Off, click), None);
        assert!(sgr(Tracking::Clicks, click).is_some());
        // Dragging: from Drags on. A move with no button: only Moves.
        let drag = Event::Move(Some(Button::Left));
        assert_eq!(sgr(Tracking::Clicks, drag), None);
        assert!(sgr(Tracking::Drags, drag).is_some());
        assert_eq!(sgr(Tracking::Drags, Event::Move(None)), None);
        assert_eq!(
            sgr(Tracking::Moves, Event::Move(None)).unwrap(),
            "\x1b[<35;4;3M"
        );
    }

    #[test]
    fn ctrl_and_alt_are_in_the_button() {
        let ctrl_alt = ModifiersState::CONTROL | ModifiersState::ALT;
        let bytes = report(
            Tracking::Clicks,
            true,
            Event::Press(Button::Left),
            ctrl_alt,
            (0, 0),
        );
        assert_eq!(bytes.unwrap(), b"\x1b[<24;1;1M");
    }

    #[test]
    fn old_way() {
        let none = ModifiersState::empty();
        let press = report(
            Tracking::Clicks,
            false,
            Event::Press(Button::Right),
            none,
            (2, 3),
        );
        assert_eq!(press.unwrap(), [0x1b, b'[', b'M', 32 + 2, 32 + 4, 32 + 3]);
        let release = report(
            Tracking::Clicks,
            false,
            Event::Release(Button::Right),
            none,
            (2, 3),
        );
        assert_eq!(release.unwrap(), [0x1b, b'[', b'M', 32 + 3, 32 + 4, 32 + 3]);
        // Col 224 and on don't fit in a byte.
        let far = report(
            Tracking::Clicks,
            false,
            Event::Press(Button::Left),
            none,
            (0, 223),
        );
        assert_eq!(far, None);
    }
}
