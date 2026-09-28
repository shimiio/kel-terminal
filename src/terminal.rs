use std::io::Write;
use std::sync::{Arc, Mutex};
use vte::{Params, Perform};

#[derive(Clone)]
pub struct Cell {
    pub ch: char,
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: ' ' }
    }
}

pub struct Grid {
    cells: Vec<Vec<Cell>>,
    rows: usize,
    cols: usize,
    cursor_row: usize,
    cursor_col: usize,
    // Set after printing in the last column. The wrap happens only when the
    // next character arrives, so a line that exactly fills the width does not
    // leave an extra blank line behind it.
    wrap_pending: bool,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
        Grid {
            cells,
            rows,
            cols,
            cursor_row: 0,
            cursor_col: 0,
            wrap_pending: false,
        }
    }

    /// Cursor position as 0-based (row, col).
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    pub fn put_char(&mut self, c: char) {
        if self.wrap_pending {
            self.carriage_return();
            self.linefeed();
        }
        self.cells[self.cursor_row][self.cursor_col].ch = c;
        if self.cursor_col + 1 < self.cols {
            self.cursor_col += 1;
        } else {
            self.wrap_pending = true;
        }
    }

    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
        self.wrap_pending = false;
    }

    /// Moves down one row, scrolling at the bottom. The column is unchanged:
    /// returning to column 0 is the job of `\r`.
    pub fn linefeed(&mut self) {
        self.wrap_pending = false;
        if self.cursor_row + 1 < self.rows {
            self.cursor_row += 1;
        } else {
            self.scroll_up();
        }
    }

    pub fn backspace(&mut self) {
        self.cursor_col = self.cursor_col.saturating_sub(1);
        self.wrap_pending = false;
    }

    /// Moves to the next tab stop. Stops are fixed every 8 columns.
    pub fn tab(&mut self) {
        let next = (self.cursor_col / 8 + 1) * 8;
        self.cursor_col = next.min(self.cols - 1);
        self.wrap_pending = false;
    }

    pub fn move_up(&mut self, n: usize) {
        self.move_to(self.cursor_row.saturating_sub(n), self.cursor_col);
    }

    pub fn move_down(&mut self, n: usize) {
        self.move_to(self.cursor_row + n, self.cursor_col);
    }

    pub fn move_forward(&mut self, n: usize) {
        self.move_to(self.cursor_row, self.cursor_col + n);
    }

    pub fn move_back(&mut self, n: usize) {
        self.move_to(self.cursor_row, self.cursor_col.saturating_sub(n));
    }

    /// Moves the cursor to a 0-based position, clamped to the screen.
    pub fn move_to(&mut self, row: usize, col: usize) {
        self.cursor_row = row.min(self.rows - 1);
        self.cursor_col = col.min(self.cols - 1);
        self.wrap_pending = false;
    }

    /// `ESC[J`: 0 = cursor to end of screen, 1 = start of screen to cursor,
    /// 2 = whole screen. The cursor does not move.
    pub fn erase_in_display(&mut self, mode: u16) {
        match mode {
            0 => {
                self.erase_in_line(0);
                for row in self.cursor_row + 1..self.rows {
                    self.clear_row(row);
                }
            }
            1 => {
                for row in 0..self.cursor_row {
                    self.clear_row(row);
                }
                self.erase_in_line(1);
            }
            2 => {
                for row in 0..self.rows {
                    self.clear_row(row);
                }
            }
            _ => {}
        }
    }

    /// `ESC[K`: 0 = cursor to end of line, 1 = start of line to cursor,
    /// 2 = whole line. The cursor does not move.
    pub fn erase_in_line(&mut self, mode: u16) {
        let col = self.cursor_col;
        let range = match mode {
            0 => col..self.cols,
            1 => 0..col + 1,
            2 => 0..self.cols,
            _ => return,
        };
        self.cells[self.cursor_row][range].fill(Cell::default());
    }

    pub fn render(&self) -> String {
        self.cells
            .iter()
            .map(|row| {
                row.iter()
                    .map(|c| c.ch)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn clear_row(&mut self, row: usize) {
        self.cells[row].fill(Cell::default());
    }

    fn scroll_up(&mut self) {
        self.cells.remove(0);
        self.cells.push(vec![Cell::default(); self.cols])
    }
}

pub struct TerminalHandler {
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub grid: Grid,
}

impl TerminalHandler {
    /// `ESC[5n` asks "are you OK?", `ESC[6n` asks "where is the cursor?".
    /// ConPTY sends `ESC[6n` at startup and waits for the answer.
    fn device_status_report(&mut self, kind: u16) {
        let reply = match kind {
            5 => "\x1b[0n".to_string(),
            6 => {
                let (row, col) = self.grid.cursor();
                format!("\x1b[{};{}R", row + 1, col + 1)
            }
            _ => return,
        };
        self.reply(reply.as_bytes());
    }

    /// Sends bytes back to the shell, as if they were typed.
    fn reply(&self, bytes: &[u8]) {
        let mut w = self.writer.lock().unwrap();
        // The shell may already be gone; there is no one to tell then.
        let _ = w.write_all(bytes);
        let _ = w.flush();
    }
}

impl Perform for TerminalHandler {
    fn print(&mut self, c: char) {
        self.grid.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => self.grid.carriage_return(),
            // LF, VT and FF all move down one line.
            b'\n' | 0x0b | 0x0c => self.grid.linefeed(),
            0x08 => self.grid.backspace(),
            b'\t' => self.grid.tab(),
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        eprintln!("{action} {params:?} {intermediates:?}");

        // Private sequences such as `ESC[?25l` carry a `?` here. They switch
        // modes, which are not supported yet.
        if !intermediates.is_empty() {
            return;
        }

        // Movement counts treat a missing or 0 parameter as 1.
        let n = arg(params, 0).max(1) as usize;

        // Escape sequences count rows and columns from 1; the grid counts
        // from 0.
        match action {
            'A' => self.grid.move_up(n),
            'B' | 'e' => self.grid.move_down(n),
            'C' | 'a' => self.grid.move_forward(n),
            'D' => self.grid.move_back(n),
            'E' => {
                self.grid.move_down(n);
                self.grid.carriage_return();
            }
            'F' => {
                self.grid.move_up(n);
                self.grid.carriage_return();
            }
            'G' | '`' => {
                let (row, _) = self.grid.cursor();
                self.grid.move_to(row, n - 1);
            }
            'd' => {
                let (_, col) = self.grid.cursor();
                self.grid.move_to(n - 1, col);
            }
            'H' | 'f' => {
                let row = arg(params, 0).max(1) as usize;
                let col = arg(params, 1).max(1) as usize;
                self.grid.move_to(row - 1, col - 1);
            }
            'J' => self.grid.erase_in_display(arg(params, 0)),
            'K' => self.grid.erase_in_line(arg(params, 0)),
            'n' => self.device_status_report(arg(params, 0)),
            _ => {}
        }
    }
}

/// Returns CSI parameter `i`, or 0 when it is missing.
fn arg(params: &Params, i: usize) -> u16 {
    params
        .iter()
        .nth(i)
        .and_then(|p| p.first().copied())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use vte::Parser;

    /// Stands in for the PTY so tests can read what the terminal replied.
    #[derive(Clone, Default)]
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn run(rows: usize, cols: usize, input: &[u8]) -> (TerminalHandler, SharedBuf) {
        let out = SharedBuf::default();
        let mut handler = TerminalHandler {
            writer: Arc::new(Mutex::new(Box::new(out.clone()))),
            grid: Grid::new(rows, cols),
        };
        Parser::new().advance(&mut handler, input);
        (handler, out)
    }

    fn screen(rows: usize, cols: usize, input: &[u8]) -> String {
        run(rows, cols, input).0.grid.render()
    }

    #[test]
    fn prints_text() {
        let (h, _) = run(2, 10, b"hello");
        assert_eq!(h.grid.render(), "hello\n");
        assert_eq!(h.grid.cursor(), (0, 5));
    }

    #[test]
    fn crlf_starts_next_line() {
        assert_eq!(screen(2, 10, b"ab\r\ncd"), "ab\ncd");
    }

    #[test]
    fn linefeed_keeps_column() {
        assert_eq!(screen(2, 10, b"ab\ncd"), "ab\n  cd");
    }

    #[test]
    fn wraps_only_when_next_char_arrives() {
        let (h, _) = run(3, 4, b"abcd");
        assert_eq!(h.grid.render(), "abcd\n\n");
        assert_eq!(h.grid.cursor(), (0, 3));

        assert_eq!(screen(3, 4, b"abcde"), "abcd\ne\n");
    }

    #[test]
    fn scrolls_at_bottom() {
        assert_eq!(screen(2, 10, b"a\r\nb\r\nc"), "b\nc");
    }

    #[test]
    fn backspace_and_tab() {
        assert_eq!(screen(1, 20, b"abc\x08X"), "abX");
        assert_eq!(screen(1, 20, b"a\tb"), "a       b");
    }

    #[test]
    fn cursor_movement() {
        // 3;3H -> (2,2), 2A -> (0,2), 2C -> (0,4)
        assert_eq!(screen(3, 5, b"\x1b[3;3H\x1b[2A\x1b[2Cx"), "    x\n\n");
        assert_eq!(screen(3, 5, b"\x1b[2;3H\x1b[Dx"), "\n x\n");
        assert_eq!(screen(3, 5, b"abc\x1b[2Ex"), "abc\n\nx");
        assert_eq!(screen(3, 5, b"\x1b[3dx"), "\n\nx");
        assert_eq!(screen(3, 5, b"\x1b[2;4Hx\x1b[2Gy"), "\n y x\n");
    }

    #[test]
    fn movement_clamps_to_screen() {
        assert_eq!(screen(3, 5, b"\x1b[99;99Hx"), "\n\n    x");
        assert_eq!(screen(3, 5, b"\x1b[9Ax"), "x\n\n");
    }

    #[test]
    fn missing_or_zero_params_mean_one() {
        assert_eq!(screen(2, 5, b"abc\x1b[Hx"), "xbc\n");
        assert_eq!(screen(2, 5, b"abc\x1b[0;0Hx"), "xbc\n");
    }

    #[test]
    fn erase_in_line() {
        assert_eq!(screen(1, 5, b"abcde\x1b[3G\x1b[K"), "ab");
        assert_eq!(screen(1, 5, b"abcde\x1b[3G\x1b[1K"), "   de");
        assert_eq!(screen(1, 5, b"abcde\x1b[3G\x1b[2K"), "");
    }

    #[test]
    fn erase_in_display() {
        let filled = b"aaa\r\nbbb\r\nccc\x1b[2;2H";
        let with = |seq: &[u8]| screen(3, 3, &[&filled[..], seq].concat());

        assert_eq!(with(b"\x1b[J"), "aaa\nb\n");
        assert_eq!(with(b"\x1b[1J"), "\n  b\nccc");
        assert_eq!(with(b"\x1b[2J"), "\n\n");
    }

    #[test]
    fn erase_does_not_move_cursor() {
        let (h, _) = run(3, 3, b"\x1b[2;2H\x1b[2J");
        assert_eq!(h.grid.cursor(), (1, 1));
    }

    #[test]
    fn reports_cursor_position() {
        let (_, out) = run(3, 5, b"\x1b[2;3H\x1b[6n");
        assert_eq!(out.0.lock().unwrap().as_slice(), b"\x1b[2;3R");
    }

    #[test]
    fn ignores_private_sequences() {
        assert_eq!(screen(1, 5, b"\x1b[?25lx"), "x");
    }
}
