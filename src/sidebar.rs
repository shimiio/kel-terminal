//! The sidebar (`≡` at the tab bar's start, or Ctrl+Shift+B): a panel over the
//! left of the window, the rest dimmed. It lists the projects, each with its
//! color, and ends with "Save the tabs as a project".
//!
//! ```text
//! ┌────────────────────────┐
//! │ kel terminal           │
//! │ PROJECTS               │
//! │ ● kel      this window │  ← chosen: lit
//! │ ● web      …\code\web  │
//! │ ────────────────────── │
//! │ + Save the tabs as a…  │
//! │                        │
//! │ Ctrl+Shift+B  Esc      │
//! └────────────────────────┘
//! ```
//!
//! Only its state and where its rows are live here: `App` sends the keys and the
//! clicks, `renderer` draws it. Both place the rows with `row_rect`, so a click
//! lands on the row that's drawn there.

use crate::keys::Action;
use crate::layout::{Rect, scaled};

/// Where the list starts, from the panel's top; a row's height; the room before
/// the "Save" row (a line goes there). Pixels at 100%.
const LIST_TOP: usize = 86;
const ROW_HEIGHT: usize = 32;
const SAVE_GAP: usize = 16;
/// The bottom of the panel, under the list: the keys' hint.
const FOOTER: usize = 48;

/// One project in the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    /// Its `color`, for its dot. `None`: a gray dot.
    pub color: Option<u32>,
    /// At the row's right end: where it is, or "this window".
    pub hint: String,
    /// It's the project of this window.
    pub current: bool,
}

pub struct Sidebar {
    projects: Vec<Entry>,
    /// The chosen row: a project, or `projects.len()` for "Save".
    selected: usize,
}

impl Sidebar {
    /// The sidebar with these projects, this window's chosen (else the first).
    pub fn new(projects: Vec<Entry>) -> Sidebar {
        let selected = projects.iter().position(|p| p.current).unwrap_or(0);
        Sidebar { projects, selected }
    }

    pub fn projects(&self) -> &[Entry] {
        &self.projects
    }

    /// The rows: every project, and "Save" last.
    pub fn rows(&self) -> usize {
        self.projects.len() + 1
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The row under the mouse becomes the chosen one.
    pub fn select(&mut self, row: usize) {
        if row < self.rows() {
            self.selected = row;
        }
    }

    /// Up (-1) or down (1). Past the last row it goes round to the first.
    pub fn move_selection(&mut self, step: isize) {
        let n = self.rows() as isize;
        self.selected = (self.selected as isize + step).rem_euclid(n) as usize;
    }

    /// What Enter (or a click) on the chosen row does: open that project, or for
    /// "Save", the palette, where the new project's name is typed.
    pub fn chosen(&self) -> Action {
        match self.projects.get(self.selected) {
            Some(project) => Action::OpenProject(project.name.clone()),
            None => Action::Palette,
        }
    }

    /// The row at (x, y) in the window, if one is there and fits in `panel`.
    pub fn row_at(&self, panel: Rect, scale: f32, x: usize, y: usize) -> Option<usize> {
        (0..self.rows()).find(|&row| {
            let r = row_rect(panel, scale, row, self.projects.len());
            fits(panel, scale, r) && r.contains(x, y)
        })
    }
}

/// Where row `row` is in the sidebar `panel`, with `projects` projects above
/// "Save". e.g. at 100%, a panel from y = 8: row 0 at y = 8 + 86, row 1 at
/// 8 + 86 + 32; with 2 projects, "Save" at 8 + 86 + 64 + 16.
pub fn row_rect(panel: Rect, scale: f32, row: usize, projects: usize) -> Rect {
    let s = |n: usize| scaled(n, scale);
    let gap = if row >= projects { s(SAVE_GAP) } else { 0 };
    Rect {
        x: panel.x + s(8),
        y: panel.y + s(LIST_TOP) + row * s(ROW_HEIGHT) + gap,
        w: panel.w.saturating_sub(s(16)),
        h: s(ROW_HEIGHT),
    }
}

/// Whether the row `r` fits above the footer. More projects than fit: the last
/// ones aren't shown (no scrolling yet).
pub fn fits(panel: Rect, scale: f32, r: Rect) -> bool {
    r.y + r.h + scaled(FOOTER, scale) <= panel.y + panel.h
}

/// Where the keys' hint goes: the footer's top.
pub fn footer_y(panel: Rect, scale: f32) -> usize {
    (panel.y + panel.h).saturating_sub(scaled(FOOTER, scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, current: bool) -> Entry {
        Entry {
            name: name.into(),
            color: None,
            hint: String::new(),
            current,
        }
    }

    #[test]
    fn this_window_s_project_is_chosen_first() {
        let sidebar = Sidebar::new(vec![entry("api", false), entry("kel", true)]);
        assert_eq!(sidebar.selected(), 1);
        assert_eq!(sidebar.chosen(), Action::OpenProject("kel".into()));
        // No project here: the first one.
        let sidebar = Sidebar::new(vec![entry("api", false)]);
        assert_eq!(sidebar.selected(), 0);
    }

    #[test]
    fn the_save_row_is_last() {
        let mut sidebar = Sidebar::new(vec![entry("api", false), entry("kel", false)]);
        assert_eq!(sidebar.rows(), 3);
        sidebar.move_selection(-1); // up from the first: "Save"
        assert_eq!(sidebar.chosen(), Action::Palette);
        sidebar.move_selection(1); // and round to the first
        assert_eq!(sidebar.chosen(), Action::OpenProject("api".into()));
        // No projects at all: only "Save".
        let sidebar = Sidebar::new(Vec::new());
        assert_eq!((sidebar.rows(), sidebar.chosen()), (1, Action::Palette));
    }

    #[test]
    fn clicks_land_on_rows() {
        let sidebar = Sidebar::new(vec![entry("api", false), entry("kel", false)]);
        let panel = Rect {
            x: 8,
            y: 8,
            w: 280,
            h: 600,
        };
        // Rows at 94..126 and 126..158, then the gap, "Save" at 174..206.
        assert_eq!(sidebar.row_at(panel, 1.0, 50, 100), Some(0));
        assert_eq!(sidebar.row_at(panel, 1.0, 50, 130), Some(1));
        assert_eq!(sidebar.row_at(panel, 1.0, 50, 165), None); // the gap
        assert_eq!(sidebar.row_at(panel, 1.0, 50, 180), Some(2));
        assert_eq!(sidebar.row_at(panel, 1.0, 50, 50), None); // the logo
        // A short panel: "Save" doesn't fit above the footer, it isn't there.
        let short = Rect { h: 200, ..panel };
        assert_eq!(sidebar.row_at(short, 1.0, 50, 180), None);
    }
}
