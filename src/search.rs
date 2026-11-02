//! Search in the output (Ctrl+Shift+F): a small box over the terminal's top
//! right corner. Every match is marked, the chosen one brighter, and the view
//! scrolls to it. Enter goes to the one before (up: older output), Shift+Enter
//! to the next one.
//!
//! Only its state lives here: what's typed, the matches, which one is chosen.
//! The grid finds them (`Grid::find`), `App` sends the keys, `renderer` draws.

use crate::term::Found;

#[derive(Default)]
pub struct Search {
    /// What's typed in the box.
    query: String,
    /// The matches, top to bottom (oldest first).
    found: Vec<Found>,
    /// The chosen match: its place in `found`.
    current: Option<usize>,
}

impl Search {
    pub fn new() -> Search {
        Search::default()
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Letters typed into the box. Then `App` finds the matches again.
    pub fn type_text(&mut self, text: &str) {
        self.query.push_str(text);
    }

    /// Backspace: one letter less.
    pub fn backspace(&mut self) {
        self.query.pop();
    }

    /// New matches. `keep_place`: the chosen one is the last one at or above the
    /// one chosen before, so typing more letters doesn't jump away from where you
    /// are. Otherwise (a new search, the rows were cut again): the newest one,
    /// at the bottom: what you look for is usually in the last output.
    pub fn set_found(&mut self, found: Vec<Found>, keep_place: bool) {
        let newest = found.len().checked_sub(1);
        self.current = match self.current().filter(|_| keep_place) {
            Some(before) => found
                .iter()
                .rposition(|f| f.start <= before.start)
                .or(newest),
            None => newest,
        };
        self.found = found;
    }

    /// The match before (`step` = -1, up) or after (1). Past the first one it goes
    /// round to the last.
    pub fn step(&mut self, step: isize) {
        let n = self.found.len() as isize;
        if let Some(current) = self.current {
            self.current = Some((current as isize + step).rem_euclid(n) as usize);
        }
    }

    /// The chosen match.
    pub fn current(&self) -> Option<Found> {
        self.found.get(self.current?).copied()
    }

    /// What the box says at its right end: "3 of 17", "no matches", or nothing
    /// while it's empty.
    pub fn status(&self) -> Option<String> {
        if self.query.is_empty() {
            return None;
        }
        Some(match self.current {
            Some(i) => format!("{} of {}", i + 1, self.found.len()),
            None => "no matches".to_string(),
        })
    }

    /// The matches on the lasting rows `top..bottom` (what's in view), each with
    /// whether it's the chosen one. Only these are looked at for every cell.
    pub fn marks(&self, top: usize, bottom: usize) -> Vec<(Found, bool)> {
        // Top to bottom, not overlapping: their ends are in order too.
        let first = self.found.partition_point(|f| f.end.0 < top);
        self.found[first..]
            .iter()
            .enumerate()
            .take_while(|(_, f)| f.start.0 < bottom)
            .map(|(i, f)| (*f, Some(first + i) == self.current))
            .collect()
    }
}

/// Whether the cell at (lasting row, col) is in one of `marks`: `Some(true)` in
/// the chosen match, `Some(false)` in another one.
pub fn mark_at(marks: &[(Found, bool)], row: usize, col: usize) -> Option<bool> {
    marks
        .iter()
        .find(|(f, _)| f.start <= (row, col) && (row, col) <= f.end)
        .map(|(_, chosen)| *chosen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(row: usize, col: usize, len: usize) -> Found {
        Found {
            start: (row, col),
            end: (row, col + len - 1),
        }
    }

    /// "err" found in rows 2, 5 and 9.
    fn three() -> Vec<Found> {
        vec![at(2, 0, 3), at(5, 4, 3), at(9, 1, 3)]
    }

    #[test]
    fn starts_at_the_newest_and_goes_up() {
        let mut search = Search::new();
        search.type_text("err");
        search.set_found(three(), false);
        assert_eq!(search.current(), Some(at(9, 1, 3)));
        assert_eq!(search.status().as_deref(), Some("3 of 3"));
        search.step(-1);
        assert_eq!(search.current(), Some(at(5, 4, 3)));
        search.step(-1);
        search.step(-1); // past the first: round to the last
        assert_eq!(search.current(), Some(at(9, 1, 3)));
        search.step(1); // and the other way
        assert_eq!(search.current(), Some(at(2, 0, 3)));
    }

    #[test]
    fn more_letters_keep_the_place() {
        let mut search = Search::new();
        search.set_found(three(), false);
        search.step(-1); // on row 5
        // "erro": row 5's is gone, rows 2 and 9 still match. The one above stays.
        search.set_found(vec![at(2, 0, 4), at(9, 1, 4)], true);
        assert_eq!(search.current(), Some(at(2, 0, 4)));
        // A fresh search: the newest again.
        search.set_found(vec![at(2, 0, 4), at(9, 1, 4)], false);
        assert_eq!(search.current(), Some(at(9, 1, 4)));
    }

    #[test]
    fn what_the_box_says() {
        let mut search = Search::new();
        assert_eq!(search.status(), None);
        search.type_text("zzz");
        search.set_found(Vec::new(), false);
        assert_eq!(search.status().as_deref(), Some("no matches"));
        search.step(1); // nothing to step to, no panic
        assert_eq!(search.current(), None);
    }

    #[test]
    fn marks_in_view() {
        let mut search = Search::new();
        search.set_found(three(), false);
        let marks = search.marks(4, 10); // rows 4-9: the 2nd and 3rd
        assert_eq!(marks, [(at(5, 4, 3), false), (at(9, 1, 3), true)]);
        assert_eq!(mark_at(&marks, 9, 2), Some(true));
        assert_eq!(mark_at(&marks, 5, 4), Some(false));
        assert_eq!(mark_at(&marks, 5, 7), None); // just past it
        // A match over two rows: the end of one, the start of the next.
        let wrapped = Found {
            start: (3, 8),
            end: (4, 1),
        };
        let marks = [(wrapped, false)];
        assert_eq!(mark_at(&marks, 3, 9), Some(false));
        assert_eq!(mark_at(&marks, 4, 0), Some(false));
        assert_eq!(mark_at(&marks, 4, 2), None);
    }
}
