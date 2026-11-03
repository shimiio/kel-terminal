//! Understands what the shell sends.
//!
//! The shell's output is text mixed with escape codes, e.g. `ESC[31m` (red text)
//! or `ESC[2;5H` (move the cursor to row 2, column 5). The `vte` parser splits the
//! bytes and calls one of our `Perform` methods for each piece:
//! - `print`: a normal character to show
//! - `execute`: a control byte like `\r` or `\n`
//! - `csi_dispatch`: an `ESC[...` sequence, with its numbers (params) and final letter (action)
//! - `osc_dispatch`: an `ESC]...` sequence, text for the terminal itself (the window
//!   title, the shell's folder, where its commands start and end)
//!
//! Each method only decides which `Grid` function to call.

use super::Grid;
use super::grid::{CursorShape, Underline};
use crate::mouse::Tracking;
use crate::theme::Color;
use vte::Perform;

/// The shell's word about a command (`ESC]133;...`, see `shell_integration`).
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// `ESC]633;E;text`: the text of the command about to start, e.g. `nvim .`.
    Command(String),
    /// `ESC]133;C`: Enter was pressed, the command runs.
    Started,
    /// `ESC]133;D;0`: the command ended; `true` if it worked (code 0, or none given).
    Ended(bool),
}

pub struct Terminal {
    pub grid: Grid,
    /// Answers to the program's questions (e.g. "where is the cursor?"), waiting
    /// to be sent back to it. The reader thread takes them after each chunk.
    pub replies: Vec<u8>,
    /// The program asked for pasted text to be marked (`ESC[?2004h`), see `paste_bytes`.
    pub bracketed_paste: bool,
    /// The program asked for the mouse (`ESC[?1002h`...), see `mouse`.
    pub mouse: Tracking,
    /// ...and for the reports written the SGR way (`ESC[?1006h`).
    pub mouse_sgr: bool,
    /// The program asked to hear when the window gets and loses the focus (`ESC[?1004h`).
    pub focus_events: bool,
    /// The window title the program asked for (`ESC]0;title BEL`), as it came.
    pub title: Option<String>,
    /// The shell's current folder, if it says (see `shell_integration`).
    pub cwd: Option<String>,
    /// Command marks, in the order they came. The tab takes them after each chunk.
    pub marks: Vec<Mark>,
}

impl Terminal {
    pub fn new(rows: usize, cols: usize) -> Self {
        Terminal {
            grid: Grid::new(rows, cols),
            replies: Vec::new(),
            bracketed_paste: false,
            mouse: Tracking::Off,
            mouse_sgr: false,
            focus_events: false,
            title: None,
            cwd: None,
            marks: Vec::new(),
        }
    }

    /// `ESC[...m`: change the pen's colors. One sequence can hold several codes,
    /// e.g. `ESC[31;44m` = red text (31) on a blue background (44).
    fn set_colors(&mut self, params: &vte::Params) {
        let mut iter = params.iter();
        while let Some(param) = iter.next() {
            let code = param.first().copied().unwrap_or(0);
            match code {
                0 => {
                    self.grid.fg = Color::Default;
                    self.grid.bg = Color::Default;
                    self.grid.bold = false;
                    self.grid.italic = false;
                    self.grid.underline = Underline::None;
                    self.grid.underline_color = Color::Default;
                    self.grid.inverse = false;
                }

                // text style
                1 => self.grid.bold = true,
                22 => self.grid.bold = false,
                3 => self.grid.italic = true,
                23 => self.grid.italic = false,
                // `4` alone, or `4:n` for the kind (nvim's LSP errors: `4:3`, curly).
                4 => {
                    self.grid.underline = match param.get(1) {
                        None | Some(1) => Underline::Single,
                        Some(0) => Underline::None,
                        Some(2) => Underline::Double,
                        Some(3) => Underline::Curly,
                        Some(4) => Underline::Dotted,
                        Some(5) => Underline::Dashed,
                        Some(_) => Underline::Single, // a kind we don't know
                    }
                }
                21 => self.grid.underline = Underline::Double,
                24 => self.grid.underline = Underline::None,
                7 => self.grid.inverse = true,
                27 => self.grid.inverse = false,

                // text color: the color's number, the theme decides how it looks
                39 => self.grid.fg = Color::Default,
                30..=37 => self.grid.fg = Color::Indexed((code - 30) as u8),
                90..=97 => self.grid.fg = Color::Indexed((code - 90 + 8) as u8),
                38 => match extended_color(param, &mut iter) {
                    Some(color) => self.grid.fg = color,
                    None => break, // a format we don't know: stop reading this sequence
                },

                // background color: same as above, but 4x / 10x / 49 / 48
                40..=47 => self.grid.bg = Color::Indexed((code - 40) as u8),
                100..=107 => self.grid.bg = Color::Indexed((code - 100 + 8) as u8),
                49 => self.grid.bg = Color::Default,
                48 => match extended_color(param, &mut iter) {
                    Some(color) => self.grid.bg = color,
                    None => break,
                },

                // underline color: 58 like 38, 59 back to the text's
                58 => match extended_color(param, &mut iter) {
                    Some(color) => self.grid.underline_color = color,
                    None => break,
                },
                59 => self.grid.underline_color = Color::Default,
                _ => {}
            }
        }
    }
}

/// Returns the param at `index`, or `default` if it's missing or 0.
/// In escape sequences an omitted param and 0 usually mean "use the default".
fn param(params: &vte::Params, index: usize, default: u16) -> u16 {
    match params.iter().nth(index).and_then(|p| p.first()) {
        Some(&0) | None => default,
        Some(&v) => v,
    }
}

/// Reads the rest of an extended color, after a 38 (text), 48 (background) or
/// 58 (underline):
/// - `5;n`: color number n of the 256-color table
/// - `2;r;g;b`: any color, as red, green and blue (0-255 each)
///
/// Written with `;` (`38;2;r;g;b`), the numbers are the next params, in `iter`.
/// Written with `:` (`58:2::255:0:0`, nvim's way), they're all in this one,
/// `param`; and `2` may have one more number before r, g, b, a "color space",
/// left empty (0).
///
/// `None` = a format we don't know.
fn extended_color(param: &[u16], iter: &mut vte::ParamsIter) -> Option<Color> {
    let rgb =
        |r: u16, g: u16, b: u16| Color::Rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32);
    if param.len() > 1 {
        return match param[1..] {
            [5, n, ..] => Some(Color::Indexed(n.min(255) as u8)),
            [2, _, r, g, b, ..] => Some(rgb(r, g, b)),
            [2, r, g, b] => Some(rgb(r, g, b)),
            _ => None,
        };
    }
    let mut next = || iter.next().and_then(|p| p.first().copied());
    match next()? {
        5 => Some(Color::Indexed(next()?.min(255) as u8)),
        2 => {
            let r = next()? as u32;
            let g = next()? as u32;
            let b = next()? as u32;
            Some(Color::Rgb((r << 16) | (g << 8) | b))
        }
        _ => None,
    }
}

impl Perform for Terminal {
    fn print(&mut self, c: char) {
        self.grid.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => self.grid.carriage_return(),
            b'\n' => self.grid.newline(),
            b'\x08' => self.grid.backspace(),
            b'\t' => self.grid.tab(),
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        // e.g. `ESC[2;5H`: params = [2, 5], action = 'H'.

        // Private modes, `ESC[?...h` on, `ESC[?...l` off: the `?` arrives in
        // `intermediates`. Several can come at once: `ESC[?1000;1006h`.
        if intermediates == [b'?'] {
            let on = match action {
                'h' => true,
                'l' => false,
                _ => return,
            };
            for mode in params.iter().filter_map(|param| param.first()) {
                match mode {
                    25 => self.grid.cursor_visible = on,
                    1049 if on => self.grid.enter_alt_screen(),
                    1049 => self.grid.leave_alt_screen(),
                    2004 => self.bracketed_paste = on,
                    1000 | 1002 | 1003 if !on => self.mouse = Tracking::Off,
                    1000 => self.mouse = Tracking::Clicks,
                    1002 => self.mouse = Tracking::Drags,
                    1003 => self.mouse = Tracking::Moves,
                    1006 => self.mouse_sgr = on,
                    1004 => self.focus_events = on,
                    _ => {}
                }
            }
            return;
        }
        // Cursor shape, `ESC[n q`.
        if intermediates == [b' '] && action == 'q' {
            self.grid.cursor_shape = match param(params, 0, 0) {
                1 | 2 => Some(CursorShape::Block),
                3 | 4 => Some(CursorShape::Underline),
                5 | 6 => Some(CursorShape::Bar),
                _ => None, // 0 or unknown: Kel's own again
            };
            return;
        }
        if !intermediates.is_empty() {
            return;
        }
        // Normal sequences. Most take one number, the count `n` (default 1).
        let n = param(params, 0, 1) as usize;
        match action {
            'A' => self.grid.move_up(n),
            'B' => self.grid.move_down(n),
            'C' => self.grid.move_forward(n),
            'D' => self.grid.move_back(n),
            'G' => self.grid.set_col(n),
            'd' => self.grid.set_row(n),
            'H' | 'f' => self
                .grid
                .move_cursor(param(params, 0, 1) as usize, param(params, 1, 1) as usize),
            // For J and K, 0 is a real mode, not "use the default".
            'J' => self.grid.erase_in_display(param(params, 0, 0)),
            'K' => self.grid.erase_in_line(param(params, 0, 0)),
            'X' => self.grid.erase_chars(n),
            'P' => self.grid.delete_chars(n),
            '@' => self.grid.insert_blanks(n),
            'm' => self.set_colors(params),
            // Questions, `ESC[5n` "are you OK?" and `ESC[6n` "where is the cursor?".
            // The answer goes back like typed text: `ESC[0n` "OK", `ESC[{row};{col}R`.
            'n' => match param(params, 0, 0) {
                5 => self.replies.extend_from_slice(b"\x1b[0n"),
                6 => {
                    let (row, col) = self.grid.cursor();
                    // The grid counts from 0, the answer from 1.
                    let answer = format!("\x1b[{};{}R", row + 1, col + 1);
                    self.replies.extend_from_slice(answer.as_bytes());
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// `ESC]0;title BEL` or `ESC]2;title BEL`: the program names the window, e.g.
    /// PowerShell at start: `ESC]0;C:\WINDOWS\...\powershell.exe BEL`.
    /// (`ESC]1;` names the window's icon, an old X11 idea: ignored.)
    ///
    /// `ESC]9;9;C:\folder ESC\`: the shell's current folder, sent before each prompt.
    ///
    /// `ESC]633;E;text`: the command's text, `\xNN` for `\`, `;`, ESC... (see `unescape`).
    /// After it, VS Code's shells may send a `;nonce`: ignored.
    ///
    /// `ESC]133;C` / `ESC]133;D;code`: a command started / ended (see `Mark`).
    /// `133;A` and `133;B` (where the prompt starts and ends) say nothing Kel needs.
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // vte splits at every ';', also inside the text: put them back.
        let text = |parts: &[&[u8]]| String::from_utf8_lossy(&parts.join(&b';')).into_owned();
        match params {
            [b"0" | b"2", title @ ..] => self.title = Some(text(title)),
            [b"9", b"9", folder @ ..] => {
                // Some shells send it in quotes (Windows Terminal's docs do).
                let folder = text(folder);
                let folder = folder.trim_matches('"');
                self.cwd = Some(folder.to_string());
            }
            [b"633", b"E", command, ..] => {
                let command = unescape(&String::from_utf8_lossy(command));
                self.marks.push(Mark::Command(command));
            }
            [b"133", b"C", ..] => self.marks.push(Mark::Started),
            [b"133", b"D", code @ ..] => {
                let ok = matches!(code, [] | [b"0", ..]);
                self.marks.push(Mark::Ended(ok));
            }
            _ => {}
        }
    }
}

/// A command's text as `633;E` sends it, back as typed: `\xNN` is the character
/// NN (hex), `\\` a `\`. e.g. `a\x3b b` → `a; b`. Anything else stays as it is.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let hex = rest.get(2..4).filter(|_| rest[1..].starts_with('x'));
        match hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
            Some(byte) => {
                out.push(byte as char);
                rest = &rest[4..];
            }
            None if rest[1..].starts_with('\\') => {
                out.push('\\');
                rest = &rest[2..];
            }
            None => {
                out.push('\\');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::{Cell, Width};
    use vte::Parser;

    /// Feeds `input` into a fresh 5x10 terminal, like bytes arriving from the shell.
    fn feed(input: &str) -> Terminal {
        let mut terminal = Terminal::new(5, 10);
        Parser::new().advance(&mut terminal, input.as_bytes());
        terminal
    }

    #[test]
    fn window_title() {
        assert_eq!(feed("\x1b]0;hello\x07").title.as_deref(), Some("hello"));
        assert_eq!(feed("\x1b]2;hello\x1b\\").title.as_deref(), Some("hello")); // ESC \ ends it too
        assert_eq!(feed("\x1b]0;a;b\x07").title.as_deref(), Some("a;b"));
        assert_eq!(feed("\x1b]1;icon\x07").title, None);
        // The title is not text on the screen.
        assert_eq!(feed("\x1b]0;hello\x07").grid.render(), "");
    }

    #[test]
    fn current_folder() {
        let cwd = |input| feed(input).cwd;
        assert_eq!(
            cwd("\x1b]9;9;C:\\Windows\x1b\\").as_deref(),
            Some("C:\\Windows")
        );
        assert_eq!(cwd("\x1b]9;9;\"C:\\a b\"\x07").as_deref(), Some("C:\\a b"));
        assert_eq!(cwd("\x1b]9;9;C:\\a;b\x07").as_deref(), Some("C:\\a;b"));
        assert_eq!(cwd("\x1b]9;4;1;50\x07"), None); // 9;4 is a progress bar, not this
        assert_eq!(feed("\x1b]9;9;C:\\Windows\x07").grid.render(), "");
    }

    #[test]
    fn command_marks() {
        use Mark::*;
        let marks = |input| feed(input).marks;
        assert_eq!(marks("\x1b]133;C\x1b\\"), [Started]);
        assert_eq!(marks("\x1b]133;D;0\x1b\\"), [Ended(true)]);
        assert_eq!(marks("\x1b]133;D;1\x07"), [Ended(false)]);
        assert_eq!(marks("\x1b]133;D\x07"), [Ended(true)]);
        assert_eq!(marks("\x1b]133;A\x07\x1b]133;B\x07"), []);
        // Both in one chunk, in order: a quick command.
        assert_eq!(
            marks("\x1b]133;C\x07ok\r\n\x1b]133;D;0\x07"),
            [Started, Ended(true)]
        );
        // The text first, as PowerShell sends it; a nonce after it is ignored.
        assert_eq!(
            marks("\x1b]633;E;nvim .\x1b\\\x1b]133;C\x1b\\"),
            [Command("nvim .".into()), Started]
        );
        assert_eq!(
            marks("\x1b]633;E;ls\x07\x1b]633;E;ls;abc123\x07"),
            [Command("ls".into()), Command("ls".into())]
        );
    }

    #[test]
    fn a_command_s_text_unescaped() {
        assert_eq!(unescape(r"a\x3b b"), "a; b");
        assert_eq!(unescape(r"C:\x5cWindows"), r"C:\Windows");
        assert_eq!(unescape(r"C:\\Windows"), r"C:\Windows");
        assert_eq!(unescape(r"one\x0atwo\x1b"), "one\ntwo\x1b");
        // Not an escape: stays.
        assert_eq!(unescape(r"\xz1 \ end\"), r"\xz1 \ end\");
        assert_eq!(unescape("кот\\x3b"), "кот;");
    }

    #[test]
    fn prints_text() {
        assert_eq!(feed("hello").grid.render(), "hello");
    }

    #[test]
    fn answers_where_the_cursor_is() {
        assert_eq!(feed("ab\x1b[6n").replies, b"\x1b[1;3R");
        assert_eq!(feed("\x1b[2;5H\x1b[6n").replies, b"\x1b[2;5R");
        assert_eq!(feed("\x1b[5n").replies, b"\x1b[0n");
        assert!(feed("hello").replies.is_empty());
    }

    #[test]
    fn mouse_and_focus_modes() {
        let terminal = feed("\x1b[?1002h\x1b[?1006h\x1b[?1004h");
        assert_eq!(terminal.mouse, Tracking::Drags);
        assert!(terminal.mouse_sgr);
        assert!(terminal.focus_events);
        // Two at once.
        let terminal = feed("\x1b[?1000;1006h");
        assert_eq!(terminal.mouse, Tracking::Clicks);
        assert!(terminal.mouse_sgr);
        // nvim turns it off while you type a `:` command.
        assert_eq!(feed("\x1b[?1002h\x1b[?1002l").mouse, Tracking::Off);
    }

    #[test]
    fn bracketed_paste_mode() {
        assert!(!feed("").bracketed_paste);
        assert!(feed("\x1b[?2004h").bracketed_paste);
        assert!(!feed("\x1b[?2004h\x1b[?2004l").bracketed_paste);
    }

    #[test]
    fn carriage_return_and_newline() {
        assert_eq!(feed("ab\r\ncd").grid.render(), "ab\ncd");
    }

    #[test]
    fn backspace_moves_left_without_erasing() {
        let t = feed("abc\x08");
        assert_eq!(t.grid.render(), "abc");
        assert_eq!(t.grid.cursor(), (0, 2));
    }

    #[test]
    fn backspace_space_backspace_erases_a_char() {
        assert_eq!(feed("abc\x08 \x08").grid.render(), "ab");
    }

    #[test]
    fn backspace_then_erase_line() {
        assert_eq!(feed("abc\x08\x1b[K").grid.render(), "ab");
    }

    #[test]
    fn tab_jumps_to_next_stop() {
        assert_eq!(feed("a\tb").grid.render(), "a       b");
    }

    #[test]
    fn cursor_movement() {
        let t = feed("\x1b[3B\x1b[4C");
        assert_eq!(t.grid.cursor(), (3, 4));
        let t = feed("\x1b[5;5H\x1b[2A\x1b[3D");
        assert_eq!(t.grid.cursor(), (2, 1));
    }

    #[test]
    fn missing_or_zero_param_means_one() {
        assert_eq!(feed("\x1b[C").grid.cursor(), (0, 1));
        assert_eq!(feed("\x1b[0C").grid.cursor(), (0, 1));
    }

    #[test]
    fn movement_is_clamped_to_grid() {
        assert_eq!(feed("\x1b[99B\x1b[99C").grid.cursor(), (4, 9));
        assert_eq!(feed("\x1b[99A\x1b[99D").grid.cursor(), (0, 0));
    }

    #[test]
    fn set_col_and_row() {
        assert_eq!(feed("\x1b[3G\x1b[2d").grid.cursor(), (1, 2));
    }

    #[test]
    fn erase_in_line_modes() {
        assert_eq!(feed("abcdef\x1b[3G\x1b[K").grid.render(), "ab");
        assert_eq!(feed("abcdef\x1b[3G\x1b[1K").grid.render(), "   def");
        assert_eq!(feed("abcdef\x1b[3G\x1b[2K").grid.render(), "");
    }

    #[test]
    fn erase_in_display_modes() {
        let lines = "aaa\r\nbbb\r\nccc";
        assert_eq!(
            feed(&format!("{lines}\x1b[2;2H\x1b[J")).grid.render(),
            "aaa\nb"
        );
        assert_eq!(
            feed(&format!("{lines}\x1b[2;2H\x1b[1J")).grid.render(),
            "\n  b\nccc"
        );
        assert_eq!(feed(&format!("{lines}\x1b[2J")).grid.render(), "");
    }

    #[test]
    fn erase_chars_keeps_the_rest() {
        assert_eq!(feed("abcdef\x1b[2G\x1b[2X").grid.render(), "a  def");
    }

    #[test]
    fn delete_chars_shifts_left() {
        assert_eq!(feed("abcdef\x1b[2G\x1b[2P").grid.render(), "adef");
    }

    #[test]
    fn insert_blanks_shifts_right() {
        assert_eq!(feed("abcdef\x1b[2G\x1b[2@").grid.render(), "a  bcdef");
    }

    #[test]
    fn wraps_at_end_of_line() {
        assert_eq!(feed("0123456789ab").grid.render(), "0123456789\nab");
    }

    #[test]
    fn private_sequences_are_ignored() {
        assert_eq!(feed("a\x1b[?25lb").grid.render(), "ab");
    }

    #[test]
    fn colors_apply_until_reset() {
        let t = feed("\x1b[31mE\x1b[0mx");
        assert_eq!(t.grid.cell(0, 0).fg, Color::Indexed(1)); // E is red
        assert_eq!(t.grid.cell(0, 1).fg, Color::Default); // x is default again
    }

    #[test]
    fn extended_color_number() {
        let t = feed("\x1b[38;5;9mhello");
        assert_eq!(t.grid.cell(0, 0).fg, Color::Indexed(9));
    }

    #[test]
    fn background_applies_until_reset() {
        let t = feed("\x1b[41mE\x1b[0mx");
        assert_eq!(t.grid.cell(0, 0).bg, Color::Indexed(1));
        assert_eq!(t.grid.cell(0, 1).bg, Color::Default);
    }

    #[test]
    fn lines_scrolled_off_are_kept() {
        let t = feed("1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7");
        assert_eq!(t.grid.scrollback().len(), 2);
        assert_eq!(t.grid.scrollback()[0].cells[0].ch, '1'); // oldest line starts with '1'
        assert_eq!(t.grid.render(), "3\n4\n5\n6\n7"); // the screen shows the rest
    }

    #[test]
    fn scrollback_keeps_only_the_text() {
        // "ab"; a red "b" on green, and green to the edge (`ESC[K` erases in
        // the pen's background); an empty line. 8 lines, 5 on screen: 3 go up.
        let t = feed("ab\r\n\x1b[31;42mb\x1b[K\x1b[0m\r\n\r\n3\r\n4\r\n5\r\n6\r\n7");
        let lines = t.grid.scrollback();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].cells.len(), 2); // "ab"
        assert_eq!(lines[1].cells.len(), 10); // the green spaces go to the edge
        assert_eq!(lines[1].cells[9].bg, Color::Indexed(2));
        assert!(lines[2].cells.is_empty()); // an empty line keeps nothing
    }

    #[test]
    fn short_lines_show_as_before() {
        let mut t = feed("ab\r\n2\r\n3\r\n4\r\n5\r\n6");
        t.grid.scroll_view(1); // up into the scrollback: "ab" is the top row
        assert_eq!(t.grid.visible_cell(0, 1).ch, 'b');
        assert_eq!(t.grid.visible_cell(0, 9), Cell::default()); // past its end: empty
    }

    #[test]
    fn erase_scrollback() {
        let t = feed("1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\x1b[3J");
        assert_eq!(t.grid.scrollback().len(), 0);
    }

    #[test]
    fn cursor_hide_and_show() {
        assert!(!feed("\x1b[?25l").grid.cursor_visible);
        assert!(feed("\x1b[?25l\x1b[?25h").grid.cursor_visible);
    }

    #[test]
    fn bold_and_italic() {
        let t = feed("\x1b[1mB\x1b[3mI\x1b[22mi\x1b[0mx");
        assert!(t.grid.cell(0, 0).bold); // B: bold
        assert!(t.grid.cell(0, 1).bold && t.grid.cell(0, 1).italic); // I: both
        assert!(!t.grid.cell(0, 2).bold && t.grid.cell(0, 2).italic); // i: bold off, italic stays
        assert!(!t.grid.cell(0, 3).italic); // x: everything reset
    }

    #[test]
    fn underline_and_inverse() {
        let t = feed("\x1b[4mU\x1b[7mR\x1b[24mr\x1b[0mx");
        let underlined = |col| t.grid.cell(0, col).underline == Underline::Single;
        assert!(underlined(0)); // U: underline
        assert!(underlined(1) && t.grid.cell(0, 1).inverse); // R: both
        assert!(!underlined(2) && t.grid.cell(0, 2).inverse); // r: underline off, inverse stays
        assert!(!underlined(3) && !t.grid.cell(0, 3).inverse); // x: everything reset
    }

    #[test]
    fn underline_kinds_and_color() {
        // What nvim sends for an LSP error: curly, red.
        let t = feed("\x1b[4:3mA\x1b[58:2::255:0:0mB\x1b[59mC\x1b[24mD\x1b[21mE\x1b[4:0mF");
        let cell = |col| t.grid.cell(0, col);
        assert_eq!(
            (cell(0).underline, cell(0).underline_color),
            (Underline::Curly, Color::Default)
        );
        assert_eq!(
            (cell(1).underline, cell(1).underline_color),
            (Underline::Curly, Color::Rgb(0xff0000))
        );
        assert_eq!(cell(2).underline_color, Color::Default); // 59
        assert_eq!(cell(3).underline, Underline::None); // 24
        assert_eq!(cell(4).underline, Underline::Double); // 21
        assert_eq!(cell(5).underline, Underline::None); // 4:0
    }

    #[test]
    fn colors_written_with_colons() {
        let t = feed("\x1b[38:2:1:2:3mA\x1b[58:5:9mB\x1b[0mC");
        assert_eq!(t.grid.cell(0, 0).fg, Color::Rgb(0x010203)); // no color space
        assert_eq!(t.grid.cell(0, 1).underline_color, Color::Indexed(9));
        assert_eq!(t.grid.cell(0, 2).underline_color, Color::Default); // 0 resets it
    }

    #[test]
    fn wide_chars_take_two_cells() {
        let t = feed("a中b");
        assert_eq!(t.grid.render(), "a中b");
        assert_eq!(t.grid.cell(0, 1).width, Width::Wide);
        assert_eq!(t.grid.cell(0, 2).width, Width::Spacer);
        assert_eq!(t.grid.cell(0, 3).ch, 'b');
        assert_eq!(t.grid.cursor(), (0, 4));
        // An accent on its own takes a cell, as in ConPTY.
        assert_eq!(feed("e\u{301}x").grid.cell(0, 2).ch, 'x');
    }

    #[test]
    fn wide_char_in_the_last_column_wraps() {
        assert_eq!(feed("123456789中").grid.render(), "123456789\n中");
    }

    #[test]
    fn writing_over_half_a_wide_char() {
        // Over the left half, or the right one: the other half goes too.
        assert_eq!(feed("中\rx").grid.render(), "x");
        assert_eq!(feed("中\x1b[2Gx").grid.render(), " x");
    }

    #[test]
    fn erasing_and_moving_wide_chars() {
        // Erasing the right half (ESC[X at col 2) erases the whole letter.
        assert_eq!(feed("中b\x1b[2G\x1b[X").grid.render(), "  b");
        // Deleting the char before it (ESC[P) moves it whole.
        assert_eq!(feed("a中\x1b[1G\x1b[P").grid.render(), "中");
        // Inserting a blank in its middle (ESC[@) breaks it: both halves go.
        assert_eq!(feed("中b\x1b[2G\x1b[@").grid.render(), "   b");
    }

    #[test]
    fn copied_text_has_no_gap_after_a_wide_char() {
        assert_eq!(feed("中b").grid.text_between((0, 0), (0, 9)), "中b");
    }

    #[test]
    fn cursor_shape() {
        assert_eq!(feed("\x1b[2 q").grid.cursor_shape, Some(CursorShape::Block));
        assert_eq!(
            feed("\x1b[4 q").grid.cursor_shape,
            Some(CursorShape::Underline)
        );
        assert_eq!(feed("\x1b[6 q").grid.cursor_shape, Some(CursorShape::Bar));
        // Nothing asked, or 0 (nvim on its way out): Kel's own.
        assert_eq!(feed("").grid.cursor_shape, None);
        assert_eq!(feed("\x1b[6 q\x1b[0 q").grid.cursor_shape, None);
    }

    #[test]
    fn alternate_screen_keeps_the_normal_one() {
        let t = feed("hello\x1b[?1049hvim\x1b[?1049l");
        assert_eq!(t.grid.render(), "hello"); // "vim" is gone, "hello" is back
        assert_eq!(t.grid.cursor(), (0, 5)); // and the cursor too
    }

    #[test]
    fn alternate_screen_has_no_scrollback() {
        let t = feed("\x1b[?1049h1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7");
        assert_eq!(t.grid.scrollback().len(), 0); // 2 lines scrolled off, none were kept
    }

    #[test]
    fn truecolor() {
        let t = feed("\x1b[38;2;255;128;0;48;2;20;22;27mx");
        assert_eq!(t.grid.cell(0, 0).fg, Color::Rgb(0xff8000)); // orange text
        assert_eq!(t.grid.cell(0, 0).bg, Color::Rgb(0x14161b)); // Neovim's dark background
    }

    #[test]
    fn colors_256() {
        let t = feed("\x1b[38;5;196ma\x1b[38;5;244mb");
        assert_eq!(t.grid.cell(0, 0).fg, Color::Indexed(196));
        assert_eq!(t.grid.cell(0, 1).fg, Color::Indexed(244));
    }

    #[test]
    fn erase_uses_the_pen_background() {
        let t = feed("\x1b[44m\x1b[K"); // blue background, then erase the line
        assert_eq!(t.grid.cell(0, 5).bg, Color::Indexed(4)); // erased cells are blue
    }

    #[test]
    fn erase_chars_uses_the_pen_background() {
        // What ConPTY sends for nvim's "-- INSERT --" line (recorded with probe_nvim).
        let t = feed("\x1b[48;2;20;22;27m\r\n\x1b[12X");
        assert_eq!(t.grid.cell(1, 0).bg, Color::Rgb(0x14161b));
        assert_eq!(t.grid.cell(1, 9).bg, Color::Rgb(0x14161b)); // last column (the test grid is 10 wide)
    }
}
