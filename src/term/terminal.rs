use super::Grid;
use super::grid::DEFAULT_BG;
use super::grid::DEFAULT_FG;
use vte::Perform;

/// The 16 basic colors: 0-7 normal (codes 30-37), 8-15 bright (codes 90-97).
const PALETTE: [u32; 16] = [
    0x0c0c0c, 0xc50f1f, 0x13a10e, 0xc19c00, 0x0037da, 0x881798, 0x3a96dd, 0xcccccc, 0x767676,
    0xe74856, 0x16c60c, 0xf9f1a5, 0x3b78ff, 0xb4009e, 0x61d6d6, 0xf2f2f2,
];

pub struct Terminal {
    pub grid: Grid,
}

impl Terminal {
    pub fn new(rows: usize, cols: usize) -> Self {
        Terminal {
            grid: Grid::new(rows, cols),
        }
    }

    fn set_colors(&mut self, params: &vte::Params) {
        let mut iter = params.iter();
        while let Some(param) = iter.next() {
            let code = param.first().copied().unwrap_or(0);
            match code {
                0 => {
                    self.grid.fg = DEFAULT_FG;
                    self.grid.bg = DEFAULT_BG;
                }

                // text color
                39 => self.grid.fg = DEFAULT_FG,
                30..=37 => self.grid.fg = PALETTE[(code - 30) as usize],
                90..=97 => self.grid.fg = PALETTE[(code - 90 + 8) as usize],
                38 => {
                    // `38;5;n`: color number n. Take the next two params ourselves.
                    let kind = iter.next().and_then(|p| p.first().copied());
                    let n = iter.next().and_then(|p| p.first().copied());
                    match (kind, n) {
                        (Some(5), Some(n)) if n < 16 => self.grid.fg = PALETTE[n as usize],
                        _ => break, // other extended formats: not supported yet
                    }
                }

                // background color: same as above, but 4x / 10x / 49 / 48
                40..=47 => self.grid.bg = PALETTE[(code - 40) as usize],
                100..=107 => self.grid.bg = PALETTE[(code - 100 + 8) as usize],
                49 => self.grid.bg = DEFAULT_BG,
                48 => {
                    let kind = iter.next().and_then(|p| p.first().copied());
                    let n = iter.next().and_then(|p| p.first().copied());
                    match (kind, n) {
                        (Some(5), Some(n)) if n < 16 => self.grid.bg = PALETTE[n as usize],
                        _ => break,
                    }
                }

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
        // Private sequences like `CSI ? 25 l` (hide cursor) aren't supported yet.
        if !intermediates.is_empty() {
            return;
        }
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
            _ => {}
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
    fn prints_text() {
        assert_eq!(feed("hello").grid.render(), "hello");
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
        assert_eq!(t.grid.cell(0, 0).fg, PALETTE[1]); // E is red
        assert_eq!(t.grid.cell(0, 1).fg, DEFAULT_FG); // x is default again
    }

    #[test]
    fn extended_color_number() {
        let t = feed("\x1b[38;5;9mhello");
        assert_eq!(t.grid.cell(0, 0).fg, PALETTE[9]);
    }

    #[test]
    fn background_applies_until_reset() {
        let t = feed("\x1b[41mE\x1b[0mx");
        assert_eq!(t.grid.cell(0, 0).bg, PALETTE[1]);
        assert_eq!(t.grid.cell(0, 1).bg, DEFAULT_BG);
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
}
