//! Help (Ctrl+Shift+/, or "Help" in the palette): all of Kel's keys, and every
//! setting with the value it has now, in one window over everything.
//!
//! ```text
//!  KEYS                                     SETTINGS
//!  Ctrl+Shift+T    New tab                  Ctrl+,          opens config.toml
//!  Ctrl+Shift+W    Close tab                font            "JetBrains Mono"
//!  ...                                      font_size       18   Ctrl+= Ctrl+-
//!  SIDEBAR                                  [glass]
//!  Shift+Del       Delete the project       effect          "none"   after a restart
//!                                  ↑↓ PgUp PgDn scroll   Esc
//! ```
//!
//! Not F1: programs have it (htop's and Midnight Commander's help, nvim's :help).
//! The keys that work everywhere come from `keys::palette_items`, the same list
//! the palette shows, so a new shortcut is here too. The settings are read from
//! the config Kel has now; what each one does at length is in the file's comments.
//! Only its lines and how far it's scrolled live here: `App` sends the keys,
//! `renderer` draws it.

use crate::config::Config;
use crate::glass::Effect;
use crate::keys;
use crate::term::CursorShape;
use std::cell::Cell;

/// A line of the help.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// A part's name: "SIDEBAR".
    Title(String),
    /// A key and what it does, or a setting, its value now and a note.
    Row {
        key: String,
        value: String,
        note: String,
        /// A color setting's color: a dot of it before the value.
        color: Option<u32>,
    },
    /// Room between two parts.
    Gap,
}

pub struct Help {
    /// The keys, the settings: side by side in a wide window, one after the
    /// other in a narrow one.
    columns: [Vec<Line>; 2],
    /// The first line shown.
    scroll: usize,
    /// How far it can scroll: the renderer knows how many lines fit, and says so
    /// when it draws (it can't change `self` then: a `Cell`).
    max_scroll: Cell<usize>,
}

impl Help {
    /// The help, its settings as `config` has them now.
    pub fn new(config: &Config) -> Help {
        Help {
            columns: [keys_lines(), settings_lines(config)],
            scroll: 0,
            max_scroll: Cell::new(usize::MAX),
        }
    }

    pub fn columns(&self) -> &[Vec<Line>; 2] {
        &self.columns
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// The renderer, as it draws: `lines` lines are there, `shown` fit.
    pub fn fit(&self, lines: usize, shown: usize) {
        self.max_scroll.set(lines.saturating_sub(shown));
    }

    /// ↑↓ (one line), PgUp PgDn (a page), the wheel: up is negative. Not past
    /// the first line, and not past the last one at the bottom.
    pub fn scroll_by(&mut self, lines: isize) {
        let to = self.scroll.saturating_add_signed(lines);
        self.scroll = to.min(self.max_scroll.get());
    }

    /// Home (the top), End (the bottom).
    pub fn jump(&mut self, to_end: bool) {
        self.scroll = if to_end { self.max_scroll.get() } else { 0 };
    }
}

fn title(text: &str) -> Line {
    Line::Title(text.to_string())
}

fn row(key: &str, value: &str, note: &str) -> Line {
    Line::Row {
        key: key.to_string(),
        value: value.to_string(),
        note: note.to_string(),
        color: None,
    }
}

/// Every key, by where it works.
fn keys_lines() -> Vec<Line> {
    let mut lines = vec![
        title("KEYS"),
        row("Ctrl+Shift+P", "The command palette", ""),
        row("Ctrl+Shift+/", "This help", ""),
    ];
    // The palette's own list, with the shortcut each has: never out of date.
    for item in keys::palette_items() {
        if item.action != keys::Action::Help {
            lines.push(row(&item.hint, &item.label, ""));
        }
    }
    lines.push(row("Ctrl+1 … Ctrl+8", "Go to tab 1 … 8", ""));
    lines.extend([
        Line::Gap,
        title("SIDEBAR"),
        row("↑↓  Enter", "Choose, open a project", ""),
        row("Alt+↑↓", "Move a project up, down", ""),
        row("F2", "Edit: name and color", ""),
        row("Del", "Close it (saved)", ""),
        row("Shift+Del", "Delete it (not its folder)", ""),
        Line::Gap,
        title("FIND IN THE OUTPUT"),
        row("Enter  ↑↓", "Next, previous", ""),
        row("Shift+Enter", "Previous", ""),
        Line::Gap,
        title("CARD"),
        row("Tab  ↑↓", "Next field", ""),
        row("←→", "A color", ""),
        row("Enter  Esc", "Save, cancel", ""),
        Line::Gap,
        title("START SCREEN"),
        row("↑↓  Enter", "Open a recent project", ""),
        row("Typing", "The shell", ""),
        Line::Gap,
        title("MOUSE"),
        row("Double-click tab", "Its card: name, command", ""),
        row("Right click tab", "The same", ""),
        row("Middle click tab", "Close it", ""),
        row("Drag a tab", "Move it", ""),
        row("Ctrl+click link", "Open it", ""),
        row("Drag a project", "Move it, in the sidebar", ""),
        row("Rest on ◫", "The sidebar, till you leave it", ""),
        Line::Gap,
        row("Esc", "Closes what's open", ""),
    ]);
    lines
}

/// Every setting, with its value now and what it's for in a few words.
fn settings_lines(config: &Config) -> Vec<Line> {
    let n = |v: f32| v.to_string(); // 18.0 → "18", 0.85 → "0.85"
    let text = |s: &str| format!("\"{s}\"");
    let layout = &config.layout;
    let glass = &config.glass;
    let effect = match glass.effect {
        Effect::None => "none",
        Effect::Acrylic => "acrylic",
        Effect::Mica => "mica",
    };
    let cursor = match config.cursor {
        CursorShape::Block => "block",
        CursorShape::Underline => "underline",
        CursorShape::Bar => "bar",
    };
    let colors = &config.colors;
    let color = |key: &str, value: u32, note: &str| Line::Row {
        key: key.to_string(),
        value: format!("#{value:06x}"),
        note: note.to_string(),
        color: Some(value),
    };
    let accent = match colors.accent {
        Some(accent) => color("accent", accent, "home's color"),
        None => row("accent", "none", "home's color: the text's"),
    };
    vec![
        title("SETTINGS"),
        row("Ctrl+,", "config.toml", "saved: applies at once"),
        Line::Gap,
        row("font", &text(&config.font), "the terminal's"),
        row("font_size", &n(config.font_size), "Ctrl+= Ctrl+- for now"),
        row(
            "shell",
            &text(&config.shell()),
            match config.shell {
                Some(_) => "new tabs",
                None => "the system's",
            },
        ),
        row("ui_scale", &n(config.ui_scale), "tabs, menus"),
        row(
            "pixel_name",
            &config.pixel_name.to_string(),
            "the name in pixels",
        ),
        row("cursor", &text(cursor), "programs can pick theirs"),
        row(
            "backdrop",
            &text(config.backdrop.name()),
            "the start screen's",
        ),
        Line::Gap,
        title("[LAYOUT]"),
        row("margin", &layout.margin.to_string(), "window edge to panel"),
        row("border", &layout.border.to_string(), "the panel's line"),
        row("padding", &layout.padding.to_string(), "line to text"),
        row("tab_height", &layout.tab_height.to_string(), ""),
        row(
            "tab_width",
            &layout.tab_width.to_string(),
            "less with many tabs",
        ),
        Line::Gap,
        title("[GLASS]"),
        row("effect", &text(effect), "acrylic, mica: restart"),
        row("opacity", &n(glass.opacity), "1 solid, 0 all glass"),
        row("grain", &n(glass.grain), "0 none, 1 strong"),
        row(
            "solid_under_apps",
            &glass.solid_under_apps.to_string(),
            "lazygit, Claude Code",
        ),
        match glass.solid_color {
            Some(c) => color("solid_color", c, "under them"),
            None => row("solid_color", "none", "the background's"),
        },
        Line::Gap,
        title("[COLORS]"),
        color("foreground", colors.foreground, "text"),
        color("background", colors.background, ""),
        color("cursor", colors.cursor, ""),
        color("selection", colors.selection, ""),
        color("frame", colors.frame, "around the panel"),
        color("border", colors.border, "the panel's line"),
        accent,
        row(
            "project_colors",
            &format!("{} colors", colors.project_colors.len()),
            "a project's, to pick",
        ),
        color("done", colors.done, "a command ended well"),
        color("failed", colors.failed, "it failed; errors"),
        row("palette", "16 colors", "the programs' colors"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys_of(lines: &[Line]) -> Vec<&str> {
        lines
            .iter()
            .filter_map(|line| match line {
                Line::Row { key, .. } => Some(key.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_shortcut_is_in_it() {
        let help = Help::new(&Config::default());
        let keys = keys_of(&help.columns()[0]);
        for item in keys::palette_items() {
            assert!(keys.contains(&item.hint.as_str()), "{}", item.label);
        }
        assert!(keys.contains(&"Ctrl+Shift+/") && keys.contains(&"Shift+Del"));
        // Once: the help's own palette line isn't listed again.
        assert_eq!(keys.iter().filter(|k| **k == "Ctrl+Shift+/").count(), 1);
    }

    #[test]
    fn settings_as_they_are_now() {
        let mut config = Config {
            font_size: 14.5,
            ..Config::default()
        };
        config.colors.accent = Some(0x569cd6);
        let help = Help::new(&config);
        let settings = &help.columns()[1];
        let value = |key: &str| {
            settings.iter().find_map(|line| match line {
                Line::Row {
                    key: k,
                    value,
                    color,
                    ..
                } if k == key => Some((value.clone(), *color)),
                _ => None,
            })
        };
        assert_eq!(value("font_size"), Some(("14.5".to_string(), None)));
        assert_eq!(
            value("font"),
            Some(("\"JetBrains Mono\"".to_string(), None))
        );
        assert_eq!(value("effect"), Some(("\"acrylic\"".to_string(), None)));
        assert_eq!(
            value("accent"),
            Some(("#569cd6".to_string(), Some(0x569cd6)))
        );
        assert_eq!(value("project_colors").unwrap().0, "8 colors");
    }

    #[test]
    fn scrolling() {
        let mut help = Help::new(&Config::default());
        help.fit(50, 20); // 50 lines, 20 fit: 30 to scroll
        help.scroll_by(-3);
        assert_eq!(help.scroll(), 0);
        help.scroll_by(25);
        assert_eq!(help.scroll(), 25);
        help.scroll_by(25);
        assert_eq!(help.scroll(), 30);
        help.jump(false);
        assert_eq!(help.scroll(), 0);
        help.jump(true);
        assert_eq!(help.scroll(), 30);
        // A bigger window: everything fits, nothing to scroll.
        help.fit(50, 60);
        help.jump(true);
        assert_eq!(help.scroll(), 0);
    }
}
