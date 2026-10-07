use super::Grid;
use vte::Perform;

pub struct Terminal {
    pub grid: Grid,
}

impl Terminal {
    pub fn new(rows: usize, cols: usize) -> Self {
        Terminal {
            grid: Grid::new(rows, cols),
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
}
