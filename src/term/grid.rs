//! The screen as a grid of cells: what is shown where, in which colors,
//! where the cursor is, and the old lines that scrolled off the top.
//!
//! Rows and columns start at 0 here. Escape codes count from 1, so the
//! functions called from escape codes (`move_cursor`, `set_col`...) subtract 1.
//!
//! A row remembers if its text goes on in the next one (`Line::wrapped`): the
//! row filled up and the next letter went on by itself. ConPTY counts on that: a
//! line longer than the window comes as one run of letters, without `\r\n`.
//! On resize the rows are joined back into those lines and cut again at the new
//! width (`reflow`).

use crate::theme::Color;
use std::collections::VecDeque;
use unicode_width::UnicodeWidthChar;

/// How many old lines to keep. Older ones are thrown away. Lines, not rows: a
/// line 3 rows long in a narrow window is still one, so a resize doesn't throw
/// any away.
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

/// One row of cells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub cells: Vec<Cell>,
    /// The text goes on in the next row: this one filled up and the next letter
    /// went on by itself (it's one line, cut at the window's edge), not `\r\n`.
    pub wrapped: bool,
}

impl Line {
    fn blank(cols: usize, cell: Cell) -> Line {
        Line {
            cells: vec![cell; cols],
            wrapped: false,
        }
    }

    /// Nothing in it: only cells exactly like an empty one.
    fn is_blank(&self) -> bool {
        self.cells.iter().all(|cell| *cell == Cell::default())
    }

    /// The cells up to the last non-empty one. A space with a background color
    /// (a bar to the edge) isn't empty: it shows.
    fn trimmed(mut cells: Vec<Cell>) -> Vec<Cell> {
        let end = cells
            .iter()
            .rposition(|cell| *cell != Cell::default())
            .map_or(0, |last| last + 1);
        cells.truncate(end);
        cells
    }
}

/// A match of a search (`Grid::find`): its first and last cell, both inside, as
/// (row, col). The rows are lasting ones (see `Grid::dropped`), so a match
/// stays on its text while new output comes and old lines go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    pub start: (usize, usize),
    pub end: (usize, usize),
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
    /// The live screen: `rows` lines, each `cols` cells. `lines[row].cells[col]`.
    lines: Vec<Line>,
    /// Lines that scrolled off the top, oldest first. Only up to their last
    /// non-empty cell (see `keep`), so most are shorter than `cols`.
    scrollback: VecDeque<Line>,
    /// How many lines end in the scrollback: its rows that aren't `wrapped`.
    scrollback_lines: usize,
    /// How many rows were thrown away from the scrollback's top so far. A row's
    /// lasting number is `dropped` + its place in scrollback + screen: it stays
    /// the same while new rows come at the bottom and old ones go at the top.
    /// e.g. a row at scrollback[9500]: after 600 rows went, it's at [8900], and
    /// `dropped` is 600: 9500 still.
    dropped: usize,
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
    saved_screen: Option<(Vec<Line>, usize, usize)>,
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
            lines: vec![Line::blank(cols, Cell::default()); rows],
            scrollback: VecDeque::new(),
            scrollback_lines: 0,
            dropped: 0,
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

    /// A new size. On the normal screen the lines are cut again at the new width
    /// (`reflow`); a program on the alternate screen (nvim) draws itself again,
    /// so its screen is only cut or filled up.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        if (rows, cols) == (self.rows, self.cols) {
            return;
        }
        if self.saved_screen.is_some() {
            self.fit(rows, cols);
        } else {
            self.reflow(rows, cols);
        }
        self.rows = rows;
        self.cols = cols;
    }

    /// Cuts or fills up each row to `cols` and the screen to `rows`, keeping the
    /// cursor's row on screen.
    fn fit(&mut self, rows: usize, cols: usize) {
        // If the cursor would end up below the new last row, drop lines from the top,
        // so the line you're typing on stays visible.
        if self.cursor_row >= rows {
            let extra = self.cursor_row + 1 - rows;
            self.lines.drain(0..extra);
            self.cursor_row -= extra;
        }
        for line in &mut self.lines {
            line.cells.resize(cols, Cell::default());
            mend_wide(&mut line.cells); // a wide letter at the new edge lost its right half
        }
        self.lines.resize(rows, Line::blank(cols, Cell::default()));
        self.cursor_col = self.cursor_col.min(cols);
    }

    /// Cuts the lines again at `cols`: a line that took 3 rows of 40 takes 2 of
    /// 60. Done the way ConPTY does it to its own copy of the screen (measured),
    /// because right after a resize it draws its screen again over Kel's, from the
    /// top, and the scrollback above must go on where that screen starts:
    ///
    /// 1. Join the rows (scrollback, then the screen) into lines: a `wrapped` row
    ///    and the next one are one line.
    /// 2. The letter in the screen's top left corner stays there: the part of its
    ///    line before it goes up into the scrollback, the screen starts with it.
    /// 3. Cut every line at `cols`.
    /// 4. If the cursor's row is now below the screen, the screen moves down: rows
    ///    from its top go into the scrollback. A taller window doesn't bring rows
    ///    back from the scrollback: empty rows at the bottom.
    fn reflow(&mut self, rows: usize, cols: usize) {
        // The screen rows in use: up to the cursor, or text below it.
        let used = self
            .lines
            .iter()
            .rposition(|line| !line.is_blank())
            .map_or(0, |last| last + 1)
            .max(self.cursor_row + 1)
            .min(self.lines.len());
        self.lines.truncate(used);
        let mut old: Vec<Line> = self
            .scrollback
            .drain(..)
            .chain(self.lines.drain(..))
            .collect();
        self.scrollback_lines = 0;
        let top_row = old.len() - used;
        let cursor_row = top_row + self.cursor_row;

        // 1. Rows → lines: (its text, the cell that fills its last row to the
        // edge, see `text_and_fill`). `top` and `cursor`: (which line, how many
        // cells into it). The cells are moved, not copied: most lines are one
        // row, and stay one.
        let mut lines: Vec<(Vec<Cell>, Cell)> = Vec::new();
        let mut current = Vec::new();
        let (mut top, mut cursor) = ((0, 0), (0, 0));
        for i in 0..old.len() {
            if i == top_row {
                top = (lines.len(), current.len());
            }
            if i == cursor_row {
                cursor = (lines.len(), current.len() + self.cursor_col);
            }
            let mut cells = std::mem::take(&mut old[i].cells);
            if old[i].wrapped {
                // A wide letter that didn't fit in the last cell went on, and left
                // that cell empty: not a space in the text.
                let next_wide = old
                    .get(i + 1)
                    .and_then(|next| next.cells.first())
                    .is_some_and(|cell| cell.width == Width::Wide);
                if next_wide && cells.last() == Some(&Cell::default()) {
                    cells.pop();
                }
                if current.is_empty() {
                    current = cells; // the line's first row: moved too
                } else {
                    current.extend(cells);
                }
            } else if current.is_empty() {
                lines.push(text_and_fill(cells)); // a line of one row
            } else {
                let (text, fill) = text_and_fill(cells);
                current.extend(text);
                lines.push((std::mem::take(&mut current), fill));
            }
        }
        if !current.is_empty() {
            lines.push((current, Cell::default())); // the last row was wrapped
        }

        // 2. Split at the top left letter: `lines` keeps what goes up.
        let mut screen = lines.split_off(top.0.min(lines.len()));
        let mut head = None;
        if let Some((first, _)) = screen.first_mut() {
            let at = top.1.min(first.len());
            if at > 0 {
                head = Some(first.drain(..at).collect::<Vec<_>>());
            }
        }

        // 3. Cut at `cols`.
        let mut above: Vec<Line> = Vec::new();
        for (cells, fill) in lines {
            above.extend(
                wrap(cells, cols, false, fill)
                    .into_iter()
                    .map(|(_, line)| line),
            );
        }
        if let Some(head) = head {
            // It goes on on the screen.
            let rows = wrap(head, cols, true, Cell::default());
            above.extend(rows.into_iter().map(|(_, line)| line));
        }
        let (cursor_line, mut cursor_at) = (cursor.0.saturating_sub(top.0), cursor.1);
        if cursor.0 == top.0 {
            cursor_at = cursor_at.saturating_sub(top.1);
        }
        let mut below: Vec<Line> = Vec::new();
        let (mut new_row, mut new_col) = (0, 0);
        for (i, (cells, fill)) in screen.into_iter().enumerate() {
            let pieces = wrap(cells, cols, false, fill);
            if i == cursor_line {
                // The last row starting at or before the cursor's cell.
                let (row, start) = pieces
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, (start, _))| *start <= cursor_at)
                    .map_or((0, 0), |(row, (start, _))| (row, *start));
                new_row = below.len() + row;
                new_col = (cursor_at - start).min(cols);
            }
            below.extend(pieces.into_iter().map(|(_, line)| line));
        }

        // 4. The cursor on screen.
        let screen_top = (new_row + 1).saturating_sub(rows);
        above.extend(below.drain(..screen_top.min(below.len())));
        for line in above {
            self.keep(line);
        }
        below.truncate(rows);
        for line in &mut below {
            line.cells.resize(cols, Cell::default());
        }
        below.resize(rows, Line::blank(cols, Cell::default()));
        self.lines = below;
        self.cursor_row = new_row - screen_top;
        self.cursor_col = new_col;
        self.scroll_offset = 0;
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    /// A cell of the live screen, ignoring scrolling. Drawing uses `visible_cell`.
    pub fn cell(&self, row: usize, col: usize) -> Cell {
        self.lines[row].cells[col]
    }

    /// Cursor position as (row, col), clamped to the visible grid.
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col.min(self.cols - 1))
    }

    /// Moves the cursor one line down. On the last line, the screen scrolls instead:
    /// the top line goes into the scrollback and a blank line appears at the bottom.
    pub fn newline(&mut self) {
        if self.cursor_row + 1 >= self.rows {
            let line = self.lines.remove(0);
            if self.saved_screen.is_none() {
                self.keep(line);
            }
            self.lines.push(Line::blank(self.cols, self.blank()));
        } else {
            self.cursor_row += 1;
        }
    }

    /// Puts `line` into the scrollback, the oldest one goes past the limit.
    fn keep(&mut self, mut line: Line) {
        // The empty cells at its end stay behind: `line 123` in a window 171
        // wide keeps 8 cells, not 171 (36 bytes each: 10 000 lines took 65 MB,
        // measured). `visible_cell` shows a missing cell as an empty one.
        // Not in a wrapped row: its text goes on, and a space at its end is a
        // space between words (`reflow` joins them).
        if !line.wrapped {
            line.cells = Line::trimmed(line.cells);
        }
        line.cells.shrink_to_fit(); // gives the memory back, `truncate` alone doesn't
        if !line.wrapped {
            self.scrollback_lines += 1;
        }
        self.scrollback.push_back(line);
        // Scrolled up, reading: the view stays on the same text while new rows
        // come (it would move up a row with each one).
        if self.scroll_offset > 0 {
            self.scroll_offset += 1;
        }
        // Past the limit: the oldest line goes, all its rows.
        while self.scrollback_lines > SCROLLBACK_LIMIT {
            match self.scrollback.pop_front() {
                Some(row) if !row.wrapped => self.scrollback_lines -= 1,
                Some(_) => {}
                None => self.scrollback_lines = 0,
            }
            self.dropped += 1;
        }
        self.scroll_offset = self.scroll_offset.min(self.scrollback.len());
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
                self.lines[self.cursor_row].cells[self.cursor_col] = self.blank();
            }
            // The line goes on in the next row.
            self.lines[self.cursor_row].wrapped = true;
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
        let line = &mut self.lines[row];
        line.cells[col] = cell;
        if wide {
            // The same colors and underline, so they reach under the whole letter.
            line.cells[col + 1] = Cell {
                ch: ' ',
                width: Width::Spacer,
                ..cell
            };
            self.cursor_col += 2;
        } else {
            self.cursor_col += 1;
        }
        // The last column written again (ConPTY redrawing the row): the row
        // ends here, unless the next letter wraps.
        if self.cursor_col >= self.cols {
            line.wrapped = false;
        }
    }

    /// If the cell at (row, col) is half of a wide letter, empties the other half:
    /// the cell is about to be written over.
    fn split_wide(&mut self, row: usize, col: usize) {
        let line = &mut self.lines[row].cells;
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

    /// The line shown at this screen row, taking the scroll offset into account.
    /// Think of scrollback + screen as one long list of lines. Example: 100 lines of
    /// scrollback, scrolled up 3. Row 0 is line 97, which is `scrollback[97]`.
    /// Row 5 is line 102, which is `lines[2]`.
    fn visible_line(&self, row: usize) -> &Line {
        let sb = self.scrollback.len();
        let index = sb - self.scroll_offset + row;
        if index < sb {
            &self.scrollback[index]
        } else {
            &self.lines[index - sb]
        }
    }

    /// The cell shown at this screen position, taking the scroll offset into account.
    pub fn visible_cell(&self, row: usize, col: usize) -> Cell {
        let line = &self.visible_line(row).cells;
        line.get(col).copied().unwrap_or_default()
    }

    /// The text on screen from `start` to `end` (row, col), in reading order.
    /// Trailing spaces are removed from each line, and lines are joined with "\r\n".
    /// A line cut at the window's edge (`wrapped`) comes out whole: a long URL
    /// copied from two rows is one URL.
    pub fn text_between(&self, start: (usize, usize), end: (usize, usize)) -> String {
        let mut text = String::new();
        for row in start.0..=end.0 {
            let first_col = if row == start.0 { start.1 } else { 0 };
            let last_col = if row == end.0 { end.1 } else { self.cols - 1 };
            let line: String = (first_col..=last_col)
                .map(|col| self.visible_cell(row, col))
                .filter(|cell| cell.width != Width::Spacer) // not "中 b" for "中b"
                .map(|cell| cell.ch)
                .collect();
            if row < end.0 && self.visible_line(row).wrapped {
                text.push_str(&line);
            } else {
                text.push_str(line.trim_end());
                if row < end.0 {
                    text.push_str("\r\n");
                }
            }
        }
        text
    }

    /// Used by tests.
    pub fn scrollback(&self) -> &VecDeque<Line> {
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

    /// The lasting number (see `dropped`) of the row at the top of the view.
    pub fn first_visible_row(&self) -> usize {
        self.dropped + self.scrollback.len() - self.scroll_offset
    }

    /// Scrolls so the lasting row `row` is in view: nothing moves if it's in view
    /// already, else it's put a third of the way down, with what came before it
    /// above.
    pub fn show_row(&mut self, row: usize) {
        let Some(index) = row.checked_sub(self.dropped) else {
            return; // thrown away
        };
        if self.saved_screen.is_some() {
            return;
        }
        let sb = self.scrollback.len();
        let top = sb - self.scroll_offset;
        if (top..top + self.rows).contains(&index) {
            return;
        }
        let new_top = index.saturating_sub(self.rows / 3);
        self.scroll_offset = sb.saturating_sub(new_top);
    }

    /// Every place `query` is in the scrollback and on the screen, top to
    /// bottom, in any case: `error` finds `Error` too. It looks in lines, not
    /// rows: a word cut at the window's edge (`err` | `or`) is found.
    pub fn find(&self, query: &str) -> Vec<Found> {
        let query: Vec<char> = query.chars().map(fold_case).collect();
        let mut found = Vec::new();
        if query.is_empty() {
            return found;
        }
        // The line so far: its letters, and for each one (row, first col, last
        // col): a wide letter takes 2 cells.
        let mut text = Vec::new();
        let mut cells = Vec::new();
        for (i, line) in self.scrollback.iter().chain(&self.lines).enumerate() {
            let row = self.dropped + i;
            for (col, cell) in line.cells.iter().enumerate() {
                match cell.width {
                    Width::Spacer => {}
                    Width::Wide => cells.push((row, col, col + 1)),
                    Width::Narrow => cells.push((row, col, col)),
                }
                if cell.width != Width::Spacer {
                    text.push(fold_case(cell.ch));
                }
            }
            if !line.wrapped {
                find_in(&text, &cells, &query, &mut found);
                text.clear();
                cells.clear();
            }
        }
        find_in(&text, &cells, &query, &mut found);
        found
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
    /// Erasing the line's end ends it there: it no longer goes on in the next row.
    pub fn erase_in_line(&mut self, mode: u16) {
        let blank = self.blank();
        let col = self.cursor_col.min(self.cols);
        let line = &mut self.lines[self.cursor_row];
        let range = match mode {
            0 => col..self.cols,
            1 => 0..(col + 1).min(self.cols),
            2 => 0..self.cols,
            _ => return,
        };
        if range.end == self.cols {
            line.wrapped = false;
        }
        line.cells[range].fill(blank);
        mend_wide(&mut line.cells);
    }

    /// `CSI n J`. 0: cursor to end of screen, 1: start of screen to cursor, 2/3: everything.
    pub fn erase_in_display(&mut self, mode: u16) {
        let blank = Line::blank(self.cols, self.blank());
        match mode {
            0 => {
                self.erase_in_line(0);
                self.lines[self.cursor_row + 1..].fill(blank);
            }
            1 => {
                self.erase_in_line(1);
                self.lines[..self.cursor_row].fill(blank);
            }
            2 => self.lines.fill(blank),
            3 => {
                self.dropped += self.scrollback.len();
                self.scrollback.clear();
                self.scrollback_lines = 0;
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
        let line = &mut self.lines[self.cursor_row];
        if end == self.cols {
            line.wrapped = false;
        }
        line.cells[start..end].fill(blank);
        mend_wide(&mut line.cells);
    }

    /// `CSI n P`: delete n chars at the cursor, shifting the rest of the line left.
    pub fn delete_chars(&mut self, n: usize) {
        let blank = self.blank();
        let start = self.cursor_col.min(self.cols - 1);
        let n = n.min(self.cols - start);
        let line = &mut self.lines[self.cursor_row];
        line.wrapped = false; // its end is now blank
        line.cells[start..].rotate_left(n);
        line.cells[self.cols - n..].fill(blank);
        mend_wide(&mut line.cells);
    }

    /// `CSI n @`: insert n blanks at the cursor, shifting the rest of the line right.
    pub fn insert_blanks(&mut self, n: usize) {
        let blank = self.blank();
        let start = self.cursor_col.min(self.cols - 1);
        let n = n.min(self.cols - start);
        let line = &mut self.lines[self.cursor_row];
        line.wrapped = false; // its end fell off
        line.cells[start..].rotate_right(n);
        line.cells[start..start + n].fill(blank);
        mend_wide(&mut line.cells);
    }

    /// The live screen as plain text, for tests. Trailing spaces and empty
    /// lines at the bottom are removed, so tests can compare short strings.
    pub fn render(&self) -> String {
        let lines: Vec<String> = self.lines.iter().map(line_text).collect();
        let last = lines.iter().rposition(|l| !l.is_empty()).unwrap_or(0);
        lines[..=last].join("\n")
    }

    /// `ESC[?1049h`: put the normal screen aside and show a blank one.
    pub fn enter_alt_screen(&mut self) {
        if self.saved_screen.is_some() {
            return; // already there
        }
        let blank = vec![Line::blank(self.cols, Cell::default()); self.rows];
        let normal = std::mem::replace(&mut self.lines, blank);
        self.saved_screen = Some((normal, self.cursor_row, self.cursor_col));
        self.scroll_offset = 0;
    }

    /// `ESC[?1049l`: throw the alternate screen away and bring the normal one back.
    pub fn leave_alt_screen(&mut self) {
        if let Some((lines, row, col)) = self.saved_screen.take() {
            // The window may have been resized while vim was open: the old screen
            // and the scrollback still have the old width. Cut them again.
            let same = lines.len() == self.rows
                && lines
                    .first()
                    .is_none_or(|line| line.cells.len() == self.cols);
            self.lines = lines;
            self.cursor_row = row;
            self.cursor_col = col;
            if !same {
                self.reflow(self.rows, self.cols);
            }
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

/// `c` as the search compares it: small letters, so `E` and `e` are the same.
fn fold_case(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// The places `query` is in one line's `text`, without overlapping: "aa" is
/// twice in "aaaa", not 3 times. `cells`: where each letter of `text` is.
fn find_in(text: &[char], cells: &[(usize, usize, usize)], query: &[char], found: &mut Vec<Found>) {
    let mut i = 0;
    while i + query.len() <= text.len() {
        if text[i..i + query.len()] == *query {
            let (first, last) = (cells[i], cells[i + query.len() - 1]);
            found.push(Found {
                start: (first.0, first.1),
                end: (last.0, last.2),
            });
            i += query.len();
        } else {
            i += 1;
        }
    }
}

/// A row's text, for tests: trailing spaces removed.
fn line_text(line: &Line) -> String {
    line.cells
        .iter()
        .filter(|cell| cell.width != Width::Spacer)
        .map(|cell| cell.ch)
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// A line's end, as ConPTY sees it (measured): spaces at its end aren't text,
/// even on a background color. That color fills the rest of the line's last row,
/// at any width: `GREEN-…44` on green, then green to the edge, is 48 letters, 2
/// rows at 25 columns (ConPTY draws `44` and 2 green spaces). Counting the green
/// spaces as text would make the line rows longer than ConPTY's, and its screen
/// would then start rows off from Kel's.
///
/// Returns the text, and the cell that fills the rest: an empty one if the line
/// ends without a color.
fn text_and_fill(mut cells: Vec<Cell>) -> (Vec<Cell>, Cell) {
    // A space shows only its background: the rest of the cell doesn't matter.
    let space = |cell: &Cell| {
        cell.ch == ' '
            && cell.width == Width::Narrow
            && !cell.inverse
            && cell.underline == Underline::None
    };
    let fill = match cells.last() {
        Some(last) if space(last) => Cell {
            bg: last.bg,
            ..Cell::default()
        },
        _ => Cell::default(),
    };
    let end = cells
        .iter()
        .rposition(|cell| !space(cell))
        .map_or(0, |last| last + 1);
    cells.truncate(end);
    (cells, fill)
}

/// Cuts one line into rows of at most `cols` cells: (where each row starts in
/// the line, the row). Every row but the last is `wrapped`; the last one too if
/// the line goes on (`goes_on`). An empty line is one empty row. `fill` fills
/// the last row to the edge (see `text_and_fill`).
fn wrap(cells: Vec<Cell>, cols: usize, goes_on: bool, fill: Cell) -> Vec<(usize, Line)> {
    let mut rows = wrap_text(cells, cols, goes_on);
    if fill != Cell::default()
        && let Some((_, last)) = rows.last_mut()
    {
        last.cells.resize(cols, fill);
    }
    rows
}

fn wrap_text(cells: Vec<Cell>, cols: usize, goes_on: bool) -> Vec<(usize, Line)> {
    if cells.len() <= cols {
        // Fits: the same cells, nothing to copy.
        let wrapped = goes_on;
        return vec![(0, Line { cells, wrapped })];
    }
    let mut rows = Vec::new();
    let mut start = 0;
    loop {
        let mut end = (start + cols).min(cells.len());
        // A wide letter whose right half would be past the edge goes on to the
        // next row, and leaves the last cell empty (as `put_char` does).
        if end < cells.len() && end > start + 1 && cells[end - 1].width == Width::Wide {
            end -= 1;
        }
        let more = end < cells.len();
        let mut row = cells[start..end].to_vec();
        if more {
            row.resize(cols, Cell::default());
        }
        mend_wide(&mut row);
        rows.push((
            start,
            Line {
                cells: row,
                wrapped: more || goes_on,
            },
        ));
        if !more {
            return rows;
        }
        start = end;
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

    /// A grid with `text` typed into it, `\n` as the shell's `\r\n`.
    fn grid(rows: usize, cols: usize, text: &str) -> Grid {
        let mut grid = Grid::new(rows, cols);
        for c in text.chars() {
            if c == '\n' {
                grid.carriage_return();
                grid.newline();
            } else {
                grid.put_char(c);
            }
        }
        grid
    }

    fn scrollback_text(grid: &Grid) -> Vec<String> {
        grid.scrollback().iter().map(line_text).collect()
    }

    #[test]
    fn a_full_row_wraps_by_itself() {
        let g = grid(3, 5, "abcdefg\nx");
        assert_eq!(g.render(), "abcde\nfg\nx");
        assert!(g.lines[0].wrapped); // "abcde" goes on with "fg"
        assert!(!g.lines[1].wrapped); // "fg" ended with \r\n
    }

    #[test]
    fn writing_the_last_column_again_ends_the_row() {
        // What ConPTY sends at the bottom of the screen: 5 letters, \r\n (to
        // scroll), back to the last one, written again, and the rest.
        let mut g = grid(2, 5, "abcde\n");
        assert!(!g.lines[0].wrapped);
        g.move_cursor(1, 5);
        g.put_char('e');
        assert!(!g.lines[0].wrapped);
        g.put_char('f'); // wraps by itself now
        assert!(g.lines[0].wrapped);
        // Erasing its end: it doesn't go on any more.
        g.move_cursor(1, 3);
        g.erase_in_line(0);
        assert!(!g.lines[0].wrapped);
    }

    #[test]
    fn narrower_and_back() {
        let mut g = grid(6, 10, "0123456789abcdef\nxy");
        g.resize(6, 4);
        assert_eq!(g.render(), "0123\n4567\n89ab\ncdef\nxy");
        assert_eq!(g.cursor(), (4, 2)); // after "xy"
        g.resize(6, 10);
        assert_eq!(g.render(), "0123456789\nabcdef\nxy");
        assert_eq!(g.cursor(), (2, 2));
        assert!(g.scrollback().is_empty());
    }

    #[test]
    fn the_scrollback_is_cut_again() {
        let mut g = grid(2, 10, "0123456789abcdef\nx\ny");
        assert_eq!(scrollback_text(&g), ["0123456789", "abcdef"]);
        g.resize(2, 20);
        assert_eq!(scrollback_text(&g), ["0123456789abcdef"]);
        g.resize(2, 4);
        assert_eq!(scrollback_text(&g), ["0123", "4567", "89ab", "cdef"]);
        assert_eq!(g.render(), "x\ny");
    }

    #[test]
    fn a_shorter_window_keeps_the_rows_above() {
        let mut g = grid(4, 10, "a\nb\nc\nd");
        g.resize(2, 10);
        assert_eq!(g.render(), "c\nd");
        assert_eq!(scrollback_text(&g), ["a", "b"]); // not thrown away
        // Taller again: they stay up there, as in ConPTY.
        g.resize(4, 10);
        assert_eq!(g.render(), "c\nd");
        assert_eq!(scrollback_text(&g), ["a", "b"]);
    }

    #[test]
    fn the_top_left_letter_stays_there() {
        // As measured: 100 b's in 8 x 40, narrowed to 25 (the screen starts at
        // b 26), widened to 60 (still at b 26). Here: 10 b's in 5 x 4.
        let mut g = grid(5, 4, "bbbbbbbbbb\n1\nP");
        assert_eq!(g.render(), "bbbb\nbbbb\nbb\n1\nP");
        g.resize(5, 3); // 4 rows of b's + 2: one goes up
        assert_eq!(scrollback_text(&g), ["bbb"]);
        assert_eq!(g.render(), "bbb\nbbb\nb\n1\nP");
        g.resize(5, 4); // the b's left: 7, from the screen's top
        assert_eq!(scrollback_text(&g), ["bbb"]);
        assert_eq!(g.render(), "bbbb\nbbb\n1\nP");
    }

    #[test]
    fn a_wide_letter_at_the_edge_goes_on() {
        let mut g = grid(4, 6, "ab中cd");
        g.resize(4, 3);
        assert_eq!(g.render(), "ab\n中c\nd");
        g.resize(4, 6); // its empty cell isn't a space
        assert_eq!(g.render(), "ab中cd");
    }

    #[test]
    fn a_background_to_the_edge_is_not_text() {
        // As ConPTY sends it: 6 letters on green, then green spaces to the edge.
        let mut g = Grid::new(4, 10);
        g.bg = Color::Indexed(2);
        for c in "GREEN-    ".chars() {
            g.put_char(c);
        }
        g.bg = Color::Default;
        g.carriage_return();
        g.newline();
        g.resize(4, 4); // 6 letters: 2 rows, not 3
        assert_eq!(g.render(), "GREE\nN-");
        assert_eq!(g.cell(1, 3).bg, Color::Indexed(2)); // green to the edge
        assert_eq!(g.cursor(), (2, 0));
        g.resize(4, 12);
        assert_eq!(g.cell(0, 11).bg, Color::Indexed(2));
    }

    #[test]
    fn colors_go_along() {
        let mut g = grid(4, 4, "abcd");
        g.bg = Color::Indexed(2);
        g.put_char('e');
        g.resize(4, 8);
        assert_eq!(g.render(), "abcde");
        assert_eq!(g.cell(0, 4).bg, Color::Indexed(2));
    }

    #[test]
    fn the_limit_counts_lines_not_rows() {
        // Each line takes 2 rows at 4 columns: 20 000 rows, still 10 000 lines.
        let text = "abcdef\n".repeat(SCROLLBACK_LIMIT + 5);
        // + 1: the last line's "abcd" went up, its "ef" is on the screen.
        let mut g = grid(2, 4, &text);
        assert_eq!(g.scrollback().len(), 2 * SCROLLBACK_LIMIT + 1);
        g.resize(2, 10); // all of them are there to join
        assert_eq!(g.scrollback().len(), SCROLLBACK_LIMIT + 1);
        assert_eq!(line_text(&g.scrollback()[0]), "abcdef");
    }

    #[test]
    fn find_in_any_case() {
        let g = grid(4, 20, "an Error\nno\nERROR, error");
        let found = g.find("error");
        let starts: Vec<_> = found.iter().map(|f| f.start).collect();
        assert_eq!(starts, [(0, 3), (2, 0), (2, 7)]);
        assert_eq!(found[0].end, (0, 7));
        assert!(g.find("").is_empty());
        assert!(g.find("warning").is_empty());
    }

    #[test]
    fn find_a_word_cut_at_the_edge() {
        // "err" at the end of row 0, "or" at the start of row 1.
        let g = grid(3, 6, "an error");
        assert_eq!(g.render(), "an err\nor");
        let found = g.find("error");
        assert_eq!(
            found,
            [Found {
                start: (0, 3),
                end: (1, 1)
            }]
        );
    }

    #[test]
    fn find_wide_letters() {
        let g = grid(2, 10, "a中文b");
        // 中 is cells 1-2, 文 3-4: the match ends on 文's second cell.
        assert_eq!(
            g.find("中文"),
            [Found {
                start: (0, 1),
                end: (0, 4)
            }]
        );
        assert_eq!(g.find("文b")[0].end, (0, 5));
    }

    #[test]
    fn found_rows_last_while_old_lines_go() {
        let text = format!("needle\n{}", "x\n".repeat(SCROLLBACK_LIMIT - 1));
        let mut g = grid(2, 10, &text);
        let before = g.find("needle");
        assert_eq!(before[0].start, (0, 0));
        // 2 more lines: "needle" and the first x are thrown away.
        for _ in 0..2 {
            g.put_char('y');
            g.carriage_return();
            g.newline();
        }
        assert!(g.find("needle").is_empty());
        // A later one keeps its number: the 'y' lines.
        let ys = g.find("y");
        assert_eq!(ys[0].start.0, SCROLLBACK_LIMIT);
    }

    #[test]
    fn show_row_scrolls_only_when_needed() {
        let text: String = (0..50).map(|i| format!("{i}\n")).collect();
        let mut g = grid(10, 10, &text);
        assert_eq!(g.first_visible_row(), 41); // rows 41-50 on screen
        g.show_row(45);
        assert_eq!(g.scroll_offset(), 0); // in view already
        g.show_row(20);
        assert_eq!(g.first_visible_row(), 17); // a third of the way down
        g.show_row(48); // back on the screen: the view goes to the bottom
        assert_eq!(g.scroll_offset(), 0);
    }

    #[test]
    fn reading_up_the_view_stays_put() {
        let text: String = (0..30).map(|i| format!("{i}\n")).collect();
        let mut g = grid(5, 10, &text);
        g.scroll_view(10);
        let top = g.first_visible_row();
        let shown = g.visible_cell(0, 0);
        for c in "new\nmore\n".chars() {
            if c == '\n' {
                g.carriage_return();
                g.newline();
            } else {
                g.put_char(c);
            }
        }
        assert_eq!(g.first_visible_row(), top);
        assert_eq!(g.visible_cell(0, 0), shown);
    }

    #[test]
    fn a_wrapped_line_is_copied_whole() {
        let g = grid(3, 10, "0123456789ab\nnext");
        assert_eq!(g.text_between((0, 0), (2, 9)), "0123456789ab\r\nnext");
    }
}
