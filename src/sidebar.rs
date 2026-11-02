//! The sidebar (`≡` at the tab bar's start, or Ctrl+Shift+B): a panel over the
//! left of the window, the rest dimmed. First `home`, Kel without a project; then
//! the projects, each with its color; last "Save the tabs as a project".
//!
//! ```text
//! ┌────────────────────────┐
//! │ kel terminal           │
//! │ ○ home                 │
//! │ PROJECTS               │
//! │ ● kel             here │  ← chosen: lit
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

/// Where `home` is, from the panel's top; where the projects start (their title
/// above them); a row's height; the room before "Save" (a line goes there); the
/// bottom, under the list, for the keys' hint. Pixels at 100%.
const HOME_TOP: usize = 52;
const LIST_TOP: usize = 120;
const ROW_HEIGHT: usize = 32;
const SAVE_GAP: usize = 16;
const FOOTER: usize = 48;

/// What's going on in a project (or home) open in the window: `App` knows, and
/// tells the sidebar each time it's drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Live {
    /// Its tabs are in this window, on screen or out of sight.
    pub open: bool,
    /// A command runs in one of its tabs (nvim, `npm run dev`).
    pub busy: bool,
    /// One of its tabs has new output, not seen yet.
    pub activity: bool,
}

/// One project in the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    /// Its `color`, for its dot. `None`: a gray dot.
    pub color: Option<u32>,
    /// At the row's right end: where it is, or "here".
    pub hint: String,
    /// It's the project on screen.
    pub current: bool,
    pub live: Live,
}

/// What a row of the sidebar is.
#[derive(Debug, PartialEq)]
pub enum Row<'a> {
    /// Kel without a project. `here`: it is on screen.
    Home {
        here: bool,
        live: Live,
    },
    Project(&'a Entry),
    /// "Save the tabs as a project".
    Save,
}

pub struct Sidebar {
    projects: Vec<Entry>,
    /// What's going on at home.
    home: Live,
    /// The chosen row: 0 is `home`, then the projects, then "Save".
    selected: usize,
}

impl Sidebar {
    /// The sidebar with these projects. This window's row is chosen: its
    /// project's, or `home` without one.
    pub fn new(projects: Vec<Entry>) -> Sidebar {
        let selected = projects.iter().position(|p| p.current).map_or(0, |i| i + 1);
        Sidebar {
            projects,
            home: Live::default(),
            selected,
        }
    }

    /// What's going on in each project and at home now: `live(Some(name))` for a
    /// project, `live(None)` for home.
    pub fn update_live(&mut self, live: impl Fn(Option<&str>) -> Live) {
        self.home = live(None);
        for project in &mut self.projects {
            project.live = live(Some(&project.name));
        }
    }

    pub fn projects(&self) -> &[Entry] {
        &self.projects
    }

    /// The rows: `home`, every project, "Save".
    pub fn rows(&self) -> usize {
        self.projects.len() + 2
    }

    /// What row `row` is.
    pub fn row(&self, row: usize) -> Row<'_> {
        match row {
            0 => Row::Home {
                here: !self.projects.iter().any(|p| p.current),
                live: self.home,
            },
            _ => match self.projects.get(row - 1) {
                Some(project) => Row::Project(project),
                None => Row::Save,
            },
        }
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

    /// What Enter (or a click) on the chosen row does: go home, open that
    /// project, or for "Save", the palette, where the new project's name is typed.
    pub fn chosen(&self) -> Action {
        match self.row(self.selected) {
            Row::Home { .. } => Action::OpenHome,
            Row::Project(project) => Action::OpenProject(project.name.clone()),
            Row::Save => Action::Palette,
        }
    }

    /// The projects' names, in the order shown.
    pub fn names(&self) -> Vec<String> {
        self.projects.iter().map(|p| p.name.clone()).collect()
    }

    /// Moves the project at place `from` to place `to` (places among the
    /// projects, not rows); the ones between shift by one. The moved one stays
    /// chosen if it was. Returns `false` if nothing moved.
    pub fn move_project(&mut self, from: usize, to: usize) -> bool {
        let n = self.projects.len();
        if from == to || from >= n || to >= n {
            return false;
        }
        let project = self.projects.remove(from);
        self.projects.insert(to, project);
        if self.selected == from + 1 {
            self.selected = to + 1;
        }
        true
    }

    /// The place among the projects for one dragged to `y`: the one there, or
    /// the first or last past the ends.
    pub fn project_slot(&self, panel: Rect, scale: f32, y: usize) -> usize {
        let n = self.projects.len();
        let first = row_rect(panel, scale, 1, n).y;
        let row_h = scaled(ROW_HEIGHT, scale).max(1);
        (y.saturating_sub(first) / row_h).min(n.saturating_sub(1))
    }

    /// The row at (x, y) in the window, if one is there and fits in `panel`.
    pub fn row_at(&self, panel: Rect, scale: f32, x: usize, y: usize) -> Option<usize> {
        (0..self.rows()).find(|&row| {
            let r = row_rect(panel, scale, row, self.projects.len());
            fits(panel, scale, r) && r.contains(x, y)
        })
    }
}

/// Where row `row` is in the sidebar `panel`, with `projects` projects. e.g. at
/// 100%, a panel from y = 8: `home` at 8 + 52; the projects at 8 + 120, 8 + 152…;
/// with 2 of them, "Save" at 8 + 120 + 64 + 16. With none, "Save" leaves a row
/// for "no projects yet".
pub fn row_rect(panel: Rect, scale: f32, row: usize, projects: usize) -> Rect {
    let s = |n: usize| scaled(n, scale);
    let y = match row {
        0 => s(HOME_TOP),
        _ if row <= projects => s(LIST_TOP) + (row - 1) * s(ROW_HEIGHT),
        _ => s(LIST_TOP) + projects.max(1) * s(ROW_HEIGHT) + s(SAVE_GAP),
    };
    Rect {
        x: panel.x + s(8),
        y: panel.y + y,
        w: panel.w.saturating_sub(s(16)),
        h: s(ROW_HEIGHT),
    }
}

/// Where the projects' title ("PROJECTS") goes: just above them.
pub fn title_y(panel: Rect, scale: f32) -> usize {
    panel.y + scaled(LIST_TOP - 24, scale)
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
            live: Live::default(),
        }
    }

    #[test]
    fn this_window_s_row_is_chosen_first() {
        let sidebar = Sidebar::new(vec![entry("api", false), entry("kel", true)]);
        assert_eq!(sidebar.selected(), 2);
        assert_eq!(sidebar.chosen(), Action::OpenProject("kel".into()));
        let home = |here| Row::Home {
            here,
            live: Live::default(),
        };
        assert_eq!(sidebar.row(0), home(false));
        // No project here: home.
        let sidebar = Sidebar::new(vec![entry("api", false)]);
        assert_eq!(sidebar.row(0), home(true));
        assert_eq!(sidebar.chosen(), Action::OpenHome);
    }

    #[test]
    fn home_first_save_last() {
        let mut sidebar = Sidebar::new(vec![entry("api", false), entry("kel", false)]);
        assert_eq!(sidebar.rows(), 4);
        assert_eq!(sidebar.row(3), Row::Save);
        sidebar.move_selection(-1); // up from home: "Save"
        assert_eq!(sidebar.chosen(), Action::Palette);
        sidebar.move_selection(1); // and round to home
        assert_eq!(sidebar.chosen(), Action::OpenHome);
        sidebar.move_selection(1);
        assert_eq!(sidebar.chosen(), Action::OpenProject("api".into()));
        // No projects at all: home and "Save".
        let sidebar = Sidebar::new(Vec::new());
        assert_eq!((sidebar.rows(), sidebar.row(1)), (2, Row::Save));
    }

    #[test]
    fn what_s_going_on() {
        let mut sidebar = Sidebar::new(vec![entry("api", false), entry("kel", true)]);
        let busy = Live {
            open: true,
            busy: true,
            activity: false,
        };
        // kel runs something; api isn't open; home is, quiet.
        sidebar.update_live(|name| match name {
            Some("kel") => busy,
            Some(_) => Live::default(),
            None => Live {
                open: true,
                ..Live::default()
            },
        });
        assert_eq!(sidebar.projects()[1].live, busy);
        assert_eq!(sidebar.projects()[0].live, Live::default());
        let Row::Home { live, .. } = sidebar.row(0) else {
            panic!("home is first");
        };
        assert!(live.open && !live.busy);
    }

    #[test]
    fn moving_a_project() {
        let names = ["api", "kel", "notes", "web"];
        let mut sidebar = Sidebar::new(names.iter().map(|n| entry(n, false)).collect());
        sidebar.select(4); // web (row 0 is home)
        assert!(sidebar.move_project(3, 0));
        assert_eq!(sidebar.names(), ["web", "api", "kel", "notes"]);
        assert_eq!(sidebar.selected(), 1); // web, still chosen
        assert!(!sidebar.move_project(0, 4)); // no place 4
        // Dragged to y: projects from 8 + 120, 32 high.
        let panel = Rect {
            x: 8,
            y: 8,
            w: 280,
            h: 600,
        };
        assert_eq!(sidebar.project_slot(panel, 1.0, 130), 0);
        assert_eq!(sidebar.project_slot(panel, 1.0, 200), 2);
        assert_eq!(sidebar.project_slot(panel, 1.0, 50), 0); // above: the first
        assert_eq!(sidebar.project_slot(panel, 1.0, 500), 3); // below: the last
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
        // home at 60..92; projects at 128..160 and 160..192; the gap; "Save"
        // at 208..240.
        let at = |y| sidebar.row_at(panel, 1.0, 50, y);
        assert_eq!(at(70), Some(0));
        assert_eq!(at(130), Some(1));
        assert_eq!(at(170), Some(2));
        assert_eq!(at(200), None); // the gap
        assert_eq!(at(220), Some(3));
        assert_eq!(at(30), None); // the logo
        assert_eq!(at(110), None); // the projects' title
        // A short panel: "Save" doesn't fit above the footer, it isn't there.
        let short = Rect { h: 230, ..panel };
        assert_eq!(sidebar.row_at(short, 1.0, 50, 220), None);
    }
}
