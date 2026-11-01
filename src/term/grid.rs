//! The screen as a grid of cells: what is shown where, in which colors,
//! where the cursor is, and the old lines that scrolled off the top.
//!
//! Rows and columns start at 0 here. Escape codes count from 1, so the
//! functions called from escape codes (`move_cursor`, `set_col`...) subtract 1.

use crate::theme::Color;
use std::collections::VecDeque;
use unicode_width::UnicodeWidthChar;

/// How many old lines to keep. Older ones are thrown away.
pub const SCROLLBACK_LIMIT: usize = 10_000;
const TAB_WIDTH: usize = 8;

/// One character on the screen, with its colors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: Underline,
    /// The underline's own color; `Default`: the text's.
    pub underline_color: Color,
    pub inverse: bool,
    /// One cell of a wide character, or a normal one.
    pub width: Width,
}

/// A wide character (emoji, Chinese) takes two cells: its own, `Wide`, and the
/// next one, `Spacer`, which shows nothing by itself: the letter in the cell
/// before reaches over it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Width {
    #[default]
    Narrow,
    Wide,
    Spacer,
}

/// How many cells `c` takes: the way ConPTY counts, or Kel and ConPTY disagree
/// on where the text after it is (ConPTY sends the letters, not where they go).
/// Measured:
///
/// - 2: emoji (😀 ✅), Chinese, Japanese, Korean, fullwidth letters (Ａ): what
///   Unicode calls "wide".
/// - 1: everything else, and also the "invisible" ones, which Unicode gives 0:
///   an accent on its own (U+301), the joiner of emoji sequences (ZWJ), FE0F.
///   ConPTY doesn't put them on the letter before, so they take a cell.
/// - 2 too: the halves of a flag (🇺 + 🇦), Unicode says 1.
pub fn char_width(c: char) -> usize {
    if ('\u{1F1E6}'..='\u{1F1FF}').contains(&c) {
        return 2;
    }
    match c.width() {
        Some(2) => 2,
        _ => 1,
    }
}

/// How a cell is underlined: `ESC[4:<n>m`, n = 1 to 5 in this order. nvim marks
/// LSP errors and spelling mistakes with `Curly`, in the color `ESC[58:...m` gives.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            italic: false,
            underline: Underline::None,
            underline_color: Color::Default,
            inverse: false,
            width: Width::Narrow,
        }
    }
}

pub struct Grid {
    /// The live screen: `rows` lines, each `cols` cells. `cells[row][col]`.
    cells: Vec<Vec<Cell>>,
    /// Lines that scrolled off the top, oldest first.
    scrollback: VecDeque<Vec<Cell>>,
    /// How many lines the view is scrolled up into the scrollback. 0 = the live screen.
    scroll_offset: usize,
    rows: usize,
    cols: usize,
    cursor_row: usize,
    // Can be equal to `cols` after printing in the last column: the wrap only
    // happens when the next char is printed ("pending wrap").
    cursor_col: usize,
    // The "pen": the colors the next printed chars get. Changed by `ESC[...m`.
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: Underline,
    pub underline_color: Color,
    pub inverse: bool,
    /// Hidden by `ESC[?25l`, shown again by `ESC[?25h`.
    pub cursor_visible: bool,
    /// Set by ESC[n q.
    pub cursor_shape: CursorShape,

    /// While the alternate screen is shown (vim, less...): the normal screen and
    /// its cursor (row, col), put aside. `None` = we are on the normal screen.
    saved_screen: Option<(Vec<Vec<Cell>>, usize, usize)>,
}

/// How the cursor looks. Apps choose it with `ESC[n q`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CursorShape {
    Block,
    Underline,
    Bar,
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
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            italic: false,
            underline: Underline::None,
            underline_color: Color::Default,
            inverse: false,
            cursor_visible: true,
            cursor_shape: CursorShape::Underline,
            saved_screen: None,
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
            mend_wide(line); // a wide letter at the new edge lost its right half
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

    /// A cell of the live screen, ignoring scrolling. Drawing uses `visible_cell`.
    pub fn cell(&self, row: usize, col: usize) -> Cell {
        self.cells[row][col]
    }

    /// Cursor position as (row, col), clamped to the visible grid.
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col.min(self.cols - 1))
    }

    /// Moves the cursor one line down. On the last line, the screen scrolls instead:
    /// the top line goes into the scrollback and a blank line appears at the bottom.
    pub fn newline(&mut self) {
        if self.cursor_row + 1 >= self.rows {
            let line = self.cells.remove(0);
            if self.saved_screen.is_none() {
                self.scrollback.push_back(line);
                if self.scrollback.len() > SCROLLBACK_LIMIT {
                    self.scrollback.pop_front();
                }
            }
            let blank = self.blank();
            self.cells.push(vec![blank; self.cols]);
        } else {
            self.cursor_row += 1;
        }
    }

    /// Writes `c` at the cursor with the pen's colors, and moves the cursor right:
    /// one cell, or two for a wide character (see `char_width`).
    pub fn put_char(&mut self, c: char) {
        let wide = char_width(c) == 2 && self.cols >= 2;
        // The last char went into the last column (pending wrap): wrap now.
        // A wide one wraps from the last column too, where only half of it would
        // fit; that cell stays empty, as in ConPTY.
        if self.cursor_col >= self.cols || wide && self.cursor_col == self.cols - 1 {
            if self.cursor_col < self.cols {
                self.split_wide(self.cursor_row, self.cursor_col);
                self.cells[self.cursor_row][self.cursor_col] = self.blank();
            }
            self.cursor_col = 0;
            self.newline();
        }
        let (row, col) = (self.cursor_row, self.cursor_col);
        // Over half of a wide letter: its other half goes too.
        self.split_wide(row, col);
        if wide {
            self.split_wide(row, col + 1);
        }
        let cell = Cell {
            ch: c,
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            underline_color: self.underline_color,
            inverse: self.inverse,
            width: if wide { Width::Wide } else { Width::Narrow },
        };
        self.cells[row][col] = cell;
        if wide {
            // The same colors and underline, so they reach under the whole letter.
            self.cells[row][col + 1] = Cell {
                ch: ' ',
                width: Width::Spacer,
                ..cell
            };
            self.cursor_col += 2;
        } else {
            self.cursor_col += 1;
        }
    }

    /// If the cell at (row, col) is half of a wide letter, empties the other half:
    /// the cell is about to be written over.
    fn split_wide(&mut self, row: usize, col: usize) {
        let line = &mut self.cells[row];
        let other = match line[col].width {
            Width::Narrow => return,
            Width::Wide => col + 1,
            Width::Spacer => col.wrapping_sub(1),
        };
        if let Some(cell) = line.get_mut(other) {
            cell.ch = ' ';
            cell.width = Width::Narrow;
        }
    }

    /// The cell shown at this screen position, taking the scroll offset into account.
    /// Think of scrollback + screen as one long list of lines. Example: 100 lines of
    /// scrollback, scrolled up 3. Row 0 is line 97, which is `scrollback[97]`.
    /// Row 5 is line 102, which is `cells[2]`.
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

    /// The text on screen from `start` to `end` (row, col), in reading order.
    /// Trailing spaces are removed from each line, and lines are joined with "\r\n".
    pub fn text_between(&self, start: (usize, usize), end: (usize, usize)) -> String {
        let mut lines = Vec::new();
        for row in start.0..=end.0 {
            let first_col = if row == start.0 { start.1 } else { 0 };
            let last_col = if row == end.0 { end.1 } else { self.cols - 1 };
            let line: String = (first_col..=last_col)
                .map(|col| self.visible_cell(row, col))
                .filter(|cell| cell.width != Width::Spacer) // not "中 b" for "中b"
                .map(|cell| cell.ch)
                .collect();
            lines.push(line.trim_end().to_string());
        }
        lines.join("\r\n")
    }

    /// Used by tests.
    pub fn scrollback(&self) -> &VecDeque<Vec<Cell>> {
        &self.scrollback
    }

    /// Scrolls the view: positive = up (into the past), negative = down.
    pub fn scroll_view(&mut self, lines: i32) {
        if self.saved_screen.is_some() {
            return;
        }
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
        let blank = self.blank();
        let col = self.cursor_col.min(self.cols);
        let line = &mut self.cells[self.cursor_row];
        let range = match mode {
            0 => col..self.cols,
            1 => 0..(col + 1).min(self.cols),
            2 => 0..self.cols,
            _ => return,
        };
        line[range].fill(blank);
        mend_wide(line);
    }

    /// `CSI n J`. 0: cursor to end of screen, 1: start of screen to cursor, 2/3: everything.
    pub fn erase_in_display(&mut self, mode: u16) {
        let blank = self.blank();
        match mode {
            0 => {
                self.erase_in_line(0);
                for row in &mut self.cells[self.cursor_row + 1..] {
                    row.fill(blank);
                }
            }
            1 => {
                self.erase_in_line(1);
                for row in &mut self.cells[..self.cursor_row] {
                    row.fill(blank);
                }
            }
            2 => {
                for row in &mut self.cells {
                    row.fill(blank);
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
        let blank = self.blank();
        let start = self.cursor_col.min(self.cols - 1);
        let end = (start + n).min(self.cols);
        let line = &mut self.cells[self.cursor_row];
        line[start..end].fill(blank);
        mend_wide(line);
    }

    /// `CSI n P`: delete n chars at the cursor, shifting the rest of the line left.
    pub fn delete_chars(&mut self, n: usize) {
        let blank = self.blank();
        let start = self.cursor_col.min(self.cols - 1);
        let n = n.min(self.cols - start);
        let line = &mut self.cells[self.cursor_row];
        line[start..].rotate_left(n);
        line[self.cols - n..].fill(blank);
        mend_wide(line);
    }

    /// `CSI n @`: insert n blanks at the cursor, shifting the rest of the line right.
    pub fn insert_blanks(&mut self, n: usize) {
        let blank = self.blank();
        let start = self.cursor_col.min(self.cols - 1);
        let n = n.min(self.cols - start);
        let line = &mut self.cells[self.cursor_row];
        line[start..].rotate_right(n);
        line[start..start + n].fill(blank);
        mend_wide(line);
    }

    /// The live screen as plain text, for tests. Trailing spaces and empty
    /// lines at the bottom are removed, so tests can compare short strings.
    pub fn render(&self) -> String {
        let lines: Vec<String> = self
            .cells
            .iter()
            .map(|row| {
                row.iter()
                    .filter(|cell| cell.width != Width::Spacer)
                    .map(|cell| cell.ch)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect();
        let last = lines.iter().rposition(|l| !l.is_empty()).unwrap_or(0);
        lines[..=last].join("\n")
    }

    /// `ESC[?1049h`: put the normal screen aside and show a blank one.
    pub fn enter_alt_screen(&mut self) {
        if self.saved_screen.is_some() {
            return; // already there
        }
        let blank = vec![vec![Cell::default(); self.cols]; self.rows];
        let normal = std::mem::replace(&mut self.cells, blank);
        self.saved_screen = Some((normal, self.cursor_row, self.cursor_col));
        self.scroll_offset = 0;
    }

    /// `ESC[?1049l`: throw the alternate screen away and bring the normal one back.
    pub fn leave_alt_screen(&mut self) {
        if let Some((cells, row, col)) = self.saved_screen.take() {
            self.cells = cells;
            self.cursor_row = row;
            self.cursor_col = col;
            // The window may have been resized while vim was open; fit the old screen to the new size.
            let (rows, cols) = (self.rows, self.cols);
            self.resize(rows, cols);
        }
    }

    /// An empty cell for erasing: a space with the pen's background color,
    /// like real terminals do ("background color erase").
    fn blank(&self) -> Cell {
        Cell {
            bg: self.bg,
            ..Cell::default()
        }
    }
}

/// After cells of `line` were erased or moved: a wide letter that lost one of
/// its halves loses the other one too, rather than showing half a letter.
fn mend_wide(line: &mut [Cell]) {
    for col in 0..line.len() {
        let alone = match line[col].width {
            Width::Narrow => false,
            Width::Wide => line
                .get(col + 1)
                .is_none_or(|next| next.width != Width::Spacer),
            Width::Spacer => col == 0 || line[col - 1].width != Width::Wide,
        };
        if alone {
            line[col].ch = ' ';
            line[col].width = Width::Narrow;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_like_conpty() {
        // (char, cells), as measured in ConPTY.
        let measured = [
            ('a', 1),
            ('\u{1F600}', 2), // 😀
            ('\u{2705}', 2),  // ✅
            ('\u{4E2D}', 2),  // 中
            ('\u{AC00}', 2),  // 가
            ('\u{FF21}', 2),  // Ａ
            ('\u{1F3FD}', 2), // skin tone
            ('\u{1F1FA}', 2), // half of a flag
            ('\u{2764}', 1),  // ❤ without FE0F
            ('\u{2600}', 1),  // ☀
            ('\u{301}', 1),   // an accent on its own
            ('\u{200D}', 1),  // ZWJ
            ('\u{FE0F}', 1),
            ('\u{E0B0}', 1), // Nerd Font powerline arrow
            ('\u{2500}', 1), // ─
            ('\u{A7}', 1),   // §
            ('\u{2192}', 1), // →
        ];
        for (c, cells) in measured {
            assert_eq!(char_width(c), cells, "U+{:04X}", c as u32);
        }
    }
}
