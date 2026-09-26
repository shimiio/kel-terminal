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
        }
    }

    pub fn put_char(&mut self, c: char) {
        if self.cursor_row < self.rows && self.cursor_col < self.cols {
            self.cells[self.cursor_row][self.cursor_col].ch = c;
        }
        self.cursor_col += 1;
        if self.cursor_col >= self.cols {
            self.newline();
        }
    }

    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    pub fn newline(&mut self) {
        self.cursor_col = 0;
        if self.cursor_row + 1 < self.rows {
            self.cursor_row += 1;
        } else {
            self.scroll_up();
        }
    }

    pub fn move_cursor_down(&mut self, n: usize) {
        self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
        self.cursor_col = 0;
    }

    pub fn move_cursor_to(&mut self, row: usize, col: usize) {
        self.cursor_row = row.saturating_sub(1).min(self.rows - 1);
        self.cursor_col = col.saturating_sub(1).min(self.cols - 1);
    }

    pub fn clear_screen(&mut self) {
        for row in self.cells.iter_mut() {
            for cell in row.iter_mut() {
                cell.ch = ' ';
            }
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
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

    fn scroll_up(&mut self) {
        self.cells.remove(0);
        self.cells.push(vec![Cell::default(); self.cols])
    }
}

pub struct TerminalHandler {
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub grid: Grid,
}

impl Perform for TerminalHandler {
    fn print(&mut self, c: char) {
        self.grid.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => self.grid.carriage_return(),
            b'\n' => self.grid.newline(),
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        match action {
            'n' => {
                let is_cpr_request = params.iter().flatten().any(|&p| p == 6);
                if is_cpr_request {
                    let mut w = self.writer.lock().unwrap();
                    w.write_all(b"\x1b[1;1R").unwrap();
                    w.flush().unwrap();
                }
            }
            'B' | 'e' => self.grid.move_cursor_down(1),
            'H' | 'f' => {
                let mut it = params.iter().flatten();
                let row = *it.next().unwrap_or(&1) as usize;
                let col = *it.next().unwrap_or(&1) as usize;
                self.grid.move_cursor_to(row, col);
            }
            'J' => self.grid.clear_screen(),
            _ => {}
        }
    }
}
