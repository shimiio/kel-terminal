//! The start screen: what home shows whenever you come to it (Kel started
//! without a project, home chosen in the sidebar, a project closed), over its
//! terminal, like Visual Studio's start window. Home is position 0: a calm place
//! to start from. Not while a command runs there: you came back to watch it.
//!
//! ```text
//!        kel▌            (the logo, its cursor blinking)
//!
//!        RECENT PROJECTS
//!        ● kel        …\kel-terminal
//!        ● web        C:\code\web
//!
//!        Type to start   ↑↓ Enter opens a project   Esc
//! ```
//!
//! The shell starts behind it at once. ↑↓ and Enter open a project; any other
//! key puts the screen away and goes to the shell, so typing just starts the
//! line. Only its state and where its rows are live here: `App` sends the keys
//! and the clicks, `renderer` draws it.

use crate::backdrop::{self, Backdrop};
use crate::keys::Action;
use crate::layout::{Rect, scaled};
use crate::sidebar::{Entry, Live};

/// How many projects it lists, the ones used last.
pub const RECENT: usize = 6;

/// The column it's drawn in: this wide (less in a narrow window). Then, from its
/// top: the projects' title, the first row; a row's height; the room before the
/// keys' hint, and the hint's line. Pixels at 100%.
const COLUMN_WIDTH: usize = 460;
const TITLE_TOP: usize = 68;
const LIST_TOP: usize = 96;
const ROW_HEIGHT: usize = 32;
const HINT_GAP: usize = 24;
const HINT_HEIGHT: usize = 20;

pub struct Start {
    projects: Vec<Entry>,
    /// Chosen with ↑↓: what Enter opens. None at first, so Enter is the shell's.
    selected: Option<usize>,
    /// Under the mouse: lit, and a click opens it. Not chosen: a mouse resting
    /// over the list shouldn't make Enter open a project.
    hovered: Option<usize>,
    /// Behind it: terrain by day, stars at night (or what the config says),
    /// new each time it shows.
    backdrop: Option<Backdrop>,
}

impl Start {
    pub fn new(projects: Vec<Entry>, backdrop: backdrop::Choice) -> Start {
        Start {
            projects,
            selected: None,
            hovered: None,
            backdrop: Backdrop::now(backdrop),
        }
    }

    pub fn projects(&self) -> &[Entry] {
        &self.projects
    }

    pub fn backdrop(&self) -> Option<&Backdrop> {
        self.backdrop.as_ref()
    }

    /// Whether row `row` is lit: chosen, or under the mouse.
    pub fn lit(&self, row: usize) -> bool {
        self.selected == Some(row) || self.hovered == Some(row)
    }

    /// Down (1) from nothing chosen: the first; up (-1): the last. Then round.
    pub fn move_selection(&mut self, step: isize) {
        let n = self.projects.len() as isize;
        if n == 0 {
            return;
        }
        let from = match self.selected {
            Some(row) => row as isize,
            None if step > 0 => -1,
            None => n,
        };
        self.selected = Some((from + step).rem_euclid(n) as usize);
    }

    /// The mouse is over `row` now (or no row): returns `true` if that's new.
    pub fn hover(&mut self, row: Option<usize>) -> bool {
        std::mem::replace(&mut self.hovered, row) != row
    }

    /// What Enter does: open the chosen project. None: nothing chosen.
    pub fn chosen(&self) -> Option<Action> {
        self.selected.and_then(|row| self.open(row))
    }

    /// Opening the project in row `row`.
    pub fn open(&self, row: usize) -> Option<Action> {
        let project = self.projects.get(row)?;
        Some(Action::OpenProject(project.name.clone()))
    }

    /// What's going on in each project (see `Sidebar::update_live`).
    pub fn update_live(&mut self, live: impl Fn(Option<&str>) -> Live) {
        for project in &mut self.projects {
            project.live = live(Some(&project.name));
        }
    }

    /// The row at (x, y) in the window, if one is drawn there.
    pub fn row_at(&self, area: Rect, scale: f32, x: usize, y: usize) -> Option<usize> {
        let col = column(area, scale, self.projects.len());
        (0..self.projects.len()).find(|&row| {
            let r = row_rect(col, scale, row);
            fits(area, r) && r.contains(x, y)
        })
    }
}

/// The column in the terminal's `area`, for `rows` projects: in the middle from
/// left to right, a third of the room above it and two thirds below, which looks
/// more in the middle than the middle. e.g. 2 projects in 1000 x 600 at 100%:
/// 460 wide at x = 270, 140 + 64 = 204 high, at y = (600 - 204) / 3 = 132.
pub fn column(area: Rect, scale: f32, rows: usize) -> Rect {
    let s = |n: usize| scaled(n, scale);
    let w = s(COLUMN_WIDTH).min(area.w);
    // With no projects, a row says so.
    let h = s(LIST_TOP) + rows.max(1) * s(ROW_HEIGHT) + s(HINT_GAP) + s(HINT_HEIGHT);
    Rect {
        x: area.x + (area.w - w) / 2,
        y: area.y + area.h.saturating_sub(h) / 3,
        w,
        h,
    }
}

/// Where the projects' title goes.
pub fn title_y(column: Rect, scale: f32) -> usize {
    column.y + scaled(TITLE_TOP, scale)
}

/// Where row `row` is in `column`.
pub fn row_rect(column: Rect, scale: f32, row: usize) -> Rect {
    let s = |n: usize| scaled(n, scale);
    Rect {
        x: column.x,
        y: column.y + s(LIST_TOP) + row * s(ROW_HEIGHT),
        w: column.w,
        h: s(ROW_HEIGHT),
    }
}

/// Where the keys' hint goes, under `rows` rows.
pub fn hint_y(column: Rect, scale: f32, rows: usize) -> usize {
    row_rect(column, scale, rows.max(1)).y + scaled(HINT_GAP, scale)
}

/// Whether the row `r` fits in the terminal's `area`. A short window: the last
/// ones aren't shown.
pub fn fits(area: Rect, r: Rect) -> bool {
    r.y + r.h <= area.y + area.h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(names: &[&str]) -> Start {
        let entry = |name: &&str| Entry {
            name: name.to_string(),
            color: None,
            hint: String::new(),
            current: false,
            live: Live::default(),
        };
        Start::new(names.iter().map(entry).collect(), backdrop::Choice::None)
    }

    #[test]
    fn enter_is_the_shell_s_until_a_project_is_chosen() {
        let mut start = start(&["kel", "web", "api"]);
        assert_eq!(start.chosen(), None);
        start.move_selection(1); // down: the first
        assert_eq!(start.chosen(), Some(Action::OpenProject("kel".into())));
        start.move_selection(-1); // up from the first: round to the last
        assert_eq!(start.chosen(), Some(Action::OpenProject("api".into())));
        // Up with nothing chosen: the last.
        let mut start2 = self::start(&["kel", "web"]);
        start2.move_selection(-1);
        assert_eq!(start2.chosen(), Some(Action::OpenProject("web".into())));
        // No projects: nothing to choose.
        let mut empty = self::start(&[]);
        empty.move_selection(1);
        assert_eq!(empty.chosen(), None);
    }

    #[test]
    fn the_mouse_lights_a_row_but_doesn_t_choose_it() {
        let mut start = start(&["kel", "web"]);
        assert!(start.hover(Some(1)));
        assert!(!start.hover(Some(1))); // nothing new
        assert!(start.lit(1) && !start.lit(0));
        assert_eq!(start.chosen(), None); // Enter is still the shell's
        assert_eq!(start.open(1), Some(Action::OpenProject("web".into())));
    }

    #[test]
    fn rows_in_the_column() {
        let start = start(&["kel", "web"]);
        let area = Rect {
            x: 0,
            y: 0,
            w: 1000,
            h: 600,
        };
        let col = column(area, 1.0, 2);
        assert_eq!((col.x, col.y, col.w, col.h), (270, 132, 460, 204));
        // Rows at 132 + 96 = 228..260 and 260..292.
        assert_eq!(start.row_at(area, 1.0, 500, 230), Some(0));
        assert_eq!(start.row_at(area, 1.0, 500, 260), Some(1));
        assert_eq!(start.row_at(area, 1.0, 500, 300), None); // the hint
        assert_eq!(start.row_at(area, 1.0, 100, 230), None); // left of it
        // A short window, 140 high: the column starts at its top; the first row
        // (96..128) fits, the second (128..160) doesn't.
        let short = Rect { h: 140, ..area };
        let col = column(short, 1.0, 2);
        assert!(fits(short, row_rect(col, 1.0, 0)));
        assert!(!fits(short, row_rect(col, 1.0, 1)));
    }
}
