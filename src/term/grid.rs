use std::collections::VecDeque;

pub const SCROLLBACK_LIMIT: usize = 10_000;
pub const DEFAULT_FG: u32 = 0xd4d4d4;
pub const DEFAULT_BG: u32 = 0x1e1e1e;
const TAB_WIDTH: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: u32,
    pub bg: u32,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
        }
    }
}

pub struct Grid {
    cells: Vec<Vec<Cell>>,
    scrollback: VecDeque<Vec<Cell>>,
    scroll_offset: usize,
    rows: usize,
    cols: usize,
    cursor_row: usize,
    // Can be equal to `cols` after printing in the last column: the wrap only
    // happens when the next char is printed ("pending wrap").
    cursor_col: usize,
    pub fg: u32,
    pub bg: u32,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        Grid {
            cells: vec![vec![Cell::default(); cols]; rows],
            scrollback: VecDeque::new(),
            scroll_offset: 0,
            rows,
            cols,
            cursor_col: 0,
            cursor_row: 0,
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
        }
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        // If the cursor would end up below the new last row, drop lines from the top,
        // so the line you're typing on stays visible.
        if self.cursor_row >= rows {
            let extra = self.cursor_row + 1 - rows;
            self.cells.drain(0..extra);
            self.cursor_row -= extra;
        }

        // Make every line `cols` long, then make the grid `rows` lines tall.
        for line in &mut self.cells {
            line.resize(cols, Cell::default());
        }
        self.cells.resize(rows, vec![Cell::default(); cols]);

        self.rows = rows;
        self.cols = cols;
        self.cursor_col = self.cursor_col.min(cols);
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn cell(&self, row: usize, col: usize) -> Cell {
        self.cells[row][col]
    }

    /// Cursor position as (row, col), clamped to the visible grid.
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col.min(self.cols - 1))
    }

    pub fn newline(&mut self) {
        if self.cursor_row + 1 >= self.rows {
            let line = self.cells.remove(0);
            self.scrollback.push_back(line);
            if self.scrollback.len() > SCROLLBACK_LIMIT {
                self.scrollback.pop_front();
            }
            self.cells.push(vec![Cell::default(); self.cols]);
        } else {
            self.cursor_row += 1;
        }
    }

    pub fn put_char(&mut self, c: char) {
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.newline();
        }
        self.cells[self.cursor_row][self.cursor_col] = Cell {
            ch: c,
            fg: self.fg,
            bg: self.bg,
        };
        self.cursor_col += 1;
    }

    /// The cell shown at this screen position, taking the scroll offset into account.
    pub fn visible_cell(&self, row: usize, col: usize) -> Cell {
        let sb = self.scrollback.len();
        let index = sb - self.scroll_offset + row;
        let line = if index < sb {
            &self.scrollback[index]
        } else {
            &self.cells[index - sb]
        };
        line.get(col).copied().unwrap_or_default()
    }

    pub fn scrollback(&self) -> &VecDeque<Vec<Cell>> {
        &self.scrollback
    }

    /// Scrolls the view: positive = up (into the past), negative = down.
    pub fn scroll_view(&mut self, lines: i32) {
        let max = self.scrollback.len() as i32;
        let new = (self.scroll_offset as i32 + lines).clamp(0, max);
        self.scroll_offset = new as usize;
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0
    }

    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    /// `\x08`: move one column left, without erasing anything.
    pub fn backspace(&mut self) {
        self.cursor_col = self.cursor_col.min(self.cols - 1).saturating_sub(1);
    }

    /// `\t`: jump to the next tab stop (every 8 columns).
    pub fn tab(&mut self) {
        let next = (self.cursor_col / TAB_WIDTH + 1) * TAB_WIDTH;
        self.cursor_col = next.min(self.cols - 1);
    }

    /// Moves to a 1-based (row, col), like `CSI row;col H`.
    pub fn move_cursor(&mut self, row: usize, col: usize) {
        self.cursor_row = row.saturating_sub(1).min(self.rows - 1);
        self.cursor_col = col.saturating_sub(1).min(self.cols - 1);
    }

    pub fn move_up(&mut self, n: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(n);
    }

    pub fn move_down(&mut self, n: usize) {
        self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
    }

    pub fn move_forward(&mut self, n: usize) {
        self.cursor_col = (self.cursor_col + n).min(self.cols - 1);
    }

    pub fn move_back(&mut self, n: usize) {
        self.cursor_col = self.cursor_col.min(self.cols - 1).saturating_sub(n);
    }

    /// Moves to a 1-based column on the current row, like `CSI col G`.
    pub fn set_col(&mut self, col: usize) {
        self.cursor_col = col.saturating_sub(1).min(self.cols - 1);
    }

    /// Moves to a 1-based row, keeping the column, like `CSI row d`.
    pub fn set_row(&mut self, row: usize) {
        self.cursor_row = row.saturating_sub(1).min(self.rows - 1);
    }

    /// `CSI n K`. 0: cursor to end of line, 1: start of line to cursor, 2: whole line.
    pub fn erase_in_line(&mut self, mode: u16) {
        let col = self.cursor_col.min(self.cols);
        let line = &mut self.cells[self.cursor_row];
        let range = match mode {
            0 => col..self.cols,
            1 => 0..(col + 1).min(self.cols),
            2 => 0..self.cols,
            _ => return,
        };
        line[range].fill(Cell::default());
    }

    /// `CSI n J`. 0: cursor to end of screen, 1: start of screen to cursor, 2/3: everything.
    pub fn erase_in_display(&mut self, mode: u16) {
        match mode {
            0 => {
                self.erase_in_line(0);
                for row in &mut self.cells[self.cursor_row + 1..] {
                    row.fill(Cell::default());
                }
            }
            1 => {
                self.erase_in_line(1);
                for row in &mut self.cells[..self.cursor_row] {
                    row.fill(Cell::default());
                }
            }
            2 => {
                for row in &mut self.cells {
                    row.fill(Cell::default());
                }
            }
            3 => {
                self.scrollback.clear();
                self.scroll_offset = 0;
            }
            _ => {}
        }
    }

    /// `CSI n X`: blank out n chars starting at the cursor, without moving anything.
    pub fn erase_chars(&mut self, n: usize) {
        let start = self.cursor_col.min(self.cols - 1);
        let end = (start + n).min(self.cols);
        self.cells[self.cursor_row][start..end].fill(Cell::default());
    }

    /// `CSI n P`: delete n chars at the cursor, shifting the rest of the line left.
    pub fn delete_chars(&mut self, n: usize) {
        let start = self.cursor_col.min(self.cols - 1);
        let n = n.min(self.cols - start);
        let line = &mut self.cells[self.cursor_row];
        line[start..].rotate_left(n);
        line[self.cols - n..].fill(Cell::default());
    }

    /// `CSI n @`: insert n blanks at the cursor, shifting the rest of the line right.
    pub fn insert_blanks(&mut self, n: usize) {
        let start = self.cursor_col.min(self.cols - 1);
        let n = n.min(self.cols - start);
        let line = &mut self.cells[self.cursor_row];
        line[start..].rotate_right(n);
        line[start..start + n].fill(Cell::default());
    }

    pub fn render(&self) -> String {
        let lines: Vec<String> = self
            .cells
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| cell.ch)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect();
        let last = lines.iter().rposition(|l| !l.is_empty()).unwrap_or(0);
        lines[..=last].join("\n")
    }
}
