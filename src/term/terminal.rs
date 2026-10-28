//! Understands what the shell sends.
//!
//! The shell's output is text mixed with escape codes, e.g. `ESC[31m` (red text)
//! or `ESC[2;5H` (move the cursor to row 2, column 5). The `vte` parser splits the
//! bytes and calls one of our `Perform` methods for each piece:
//! - `print`: a normal character to show
//! - `execute`: a control byte like `\r` or `\n`
//! - `csi_dispatch`: an `ESC[...` sequence, with its numbers (params) and final letter (action)
//! - `osc_dispatch`: an `ESC]...` sequence, text for the terminal itself (the window title)
//!
//! Each method only decides which `Grid` function to call.

use super::Grid;
use super::grid::CursorShape;
use crate::theme::Color;
use vte::Perform;

pub struct Terminal {
    pub grid: Grid,
    /// Answers to the program's questions (e.g. "where is the cursor?"), waiting
    /// to be sent back to it. The reader thread takes them after each chunk.
    pub replies: Vec<u8>,
    /// The program asked for pasted text to be marked (`ESC[?2004h`), see `paste_bytes`.
    pub bracketed_paste: bool,
    /// The window title the program asked for (`ESC]0;title BEL`), as it came.
    pub title: Option<String>,
}

impl Terminal {
    pub fn new(rows: usize, cols: usize) -> Self {
        Terminal {
            grid: Grid::new(rows, cols),
            replies: Vec::new(),
            bracketed_paste: false,
            title: None,
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
                    self.grid.underline = false;
                    self.grid.inverse = false;
                }

                // text style
                1 => self.grid.bold = true,
                22 => self.grid.bold = false,
                3 => self.grid.italic = true,
                23 => self.grid.italic = false,
                4 => self.grid.underline = true,
                24 => self.grid.underline = false,
                7 => self.grid.inverse = true,
                27 => self.grid.inverse = false,

                // text color: the color's number, the theme decides how it looks
                39 => self.grid.fg = Color::Default,
                30..=37 => self.grid.fg = Color::Indexed((code - 30) as u8),
                90..=97 => self.grid.fg = Color::Indexed((code - 90 + 8) as u8),
                38 => match extended_color(&mut iter) {
                    Some(color) => self.grid.fg = color,
                    None => break, // a format we don't know: stop reading this sequence
                },

                // background color: same as above, but 4x / 10x / 49 / 48
                40..=47 => self.grid.bg = Color::Indexed((code - 40) as u8),
                100..=107 => self.grid.bg = Color::Indexed((code - 100 + 8) as u8),
                49 => self.grid.bg = Color::Default,
                48 => match extended_color(&mut iter) {
                    Some(color) => self.grid.bg = color,
                    None => break,
                },
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

/// Reads the rest of an extended color, after a 38 (text) or 48 (background):
/// - `5;n`: color number n of the 256-color table
/// - `2;r;g;b`: any color, as red, green and blue (0-255 each)
///
/// `None` = a format we don't know.
fn extended_color(iter: &mut vte::ParamsIter) -> Option<Color> {
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

        // Private sequences, `ESC[?...`: the `?` arrives in `intermediates`.
        if intermediates == [b'?'] {
            match (action, param(params, 0, 0)) {
                ('l', 25) => self.grid.cursor_visible = false,
                ('h', 25) => self.grid.cursor_visible = true,
                ('h', 1049) => self.grid.enter_alt_screen(),
                ('l', 1049) => self.grid.leave_alt_screen(),
                ('h', 2004) => self.bracketed_paste = true,
                ('l', 2004) => self.bracketed_paste = false,
                _ => {}
            }
            return;
        }
        // Cursor shape, `ESC[n q`.
        if intermediates == [b' '] && action == 'q' {
            self.grid.cursor_shape = match param(params, 0, 0) {
                1 | 2 => CursorShape::Block,
                3 | 4 => CursorShape::Underline,
                5 | 6 => CursorShape::Bar,
                _ => CursorShape::Underline, // 0 or unknown: Kel's default
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
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if let [b"0" | b"2", title @ ..] = params {
            // vte splits at every ';', also inside the title: put them back.
            self.title = Some(String::from_utf8_lossy(&title.join(&b';')).into_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(t.grid.scrollback()[0][0].ch, '1'); // oldest line starts with '1'
        assert_eq!(t.grid.render(), "3\n4\n5\n6\n7"); // the screen shows the rest
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
        assert!(t.grid.cell(0, 0).underline); // U: underline
        assert!(t.grid.cell(0, 1).underline && t.grid.cell(0, 1).inverse); // R: both
        assert!(!t.grid.cell(0, 2).underline && t.grid.cell(0, 2).inverse); // r: underline off, inverse stays
        assert!(!t.grid.cell(0, 3).underline && !t.grid.cell(0, 3).inverse); // x: everything reset
    }

    #[test]
    fn cursor_shape() {
        assert_eq!(feed("\x1b[2 q").grid.cursor_shape, CursorShape::Block);
        assert_eq!(feed("\x1b[4 q").grid.cursor_shape, CursorShape::Underline);
        assert_eq!(feed("\x1b[6 q").grid.cursor_shape, CursorShape::Bar);
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
