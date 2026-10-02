pub struct Grid {
    cells: Vec<Vec<char>>,
    rows: usize,
    cols: usize,
    cursor_row: usize,
    cursor_col: usize,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        Grid {
            cells: vec![vec![' '; cols]; rows],
            rows,
            cols,
            cursor_col: 0,
            cursor_row: 0,
        }
    }

    pub fn newline(&mut self) {
        if self.cursor_row + 1 >= self.rows {
            self.cells.remove(0);
            self.cells.push(vec![' '; self.cols]);
        } else {
            self.cursor_row += 1;
        }
    }

    pub fn put_char(&mut self, c: char) {
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.newline();
        }
        self.cells[self.cursor_row][self.cursor_col] = c;
        self.cursor_col += 1;
    }

    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    pub fn move_cursor(&mut self, row: usize, col: usize) {
        self.cursor_row = row.saturating_sub(1).min(self.rows - 1);
        self.cursor_col = col.saturating_sub(1).min(self.cols - 1);
    }

    pub fn render(&self) -> String {
        let lines: Vec<String> = self
            .cells
            .iter()
            .map(|row| row.iter().collect::<String>().trim_end().to_string())
            .collect();
        let last = lines.iter().rposition(|l| !l.is_empty()).unwrap_or(0);
        lines[..=last].join("\n")
    }
}
