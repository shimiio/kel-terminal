//! The command palette (Ctrl+Shift+P): a search box over the terminal. Type a few
//! letters, the list narrows down, Enter does the chosen thing.
//!
//! Only its state lives here: what's typed, what's shown, which line is chosen.
//! `App` sends it the keys, `renderer` draws it on top of everything else.

use crate::keys::Action;

/// One line of the list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    /// What it does, e.g. "New tab". What the search looks in.
    pub label: &'static str,
    /// Shown dimmer at the right end, e.g. the shortcut "Ctrl+Shift+T".
    pub hint: &'static str,
    pub action: Action,
}

pub struct Palette {
    /// What's typed in the search box.
    query: String,
    /// Every line, in the order they're listed.
    items: Vec<Item>,
    /// The lines that match the query: their places in `items`.
    shown: Vec<usize>,
    /// The chosen line: its place in `shown`.
    selected: usize,
}

impl Palette {
    /// A palette with an empty search box: every item shown, the first one chosen.
    pub fn new(items: Vec<Item>) -> Palette {
        let shown = (0..items.len()).collect();
        Palette {
            query: String::new(),
            items,
            shown,
            selected: 0,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// The lines that match, in order.
    pub fn shown(&self) -> impl Iterator<Item = &Item> {
        self.shown.iter().map(|&i| &self.items[i])
    }

    /// The chosen line's place among the shown ones.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Letters typed into the search box.
    pub fn type_text(&mut self, text: &str) {
        self.query.push_str(text);
        self.filter();
    }

    /// Backspace: one letter less.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.filter();
    }

    /// Up (`step` = -1) or down (1). Past the last line it goes round to the first.
    pub fn move_selection(&mut self, step: isize) {
        let n = self.shown.len() as isize;
        if n > 0 {
            self.selected = (self.selected as isize + step).rem_euclid(n) as usize;
        }
    }

    /// What Enter would do now. `None` when nothing matches.
    pub fn chosen(&self) -> Option<Action> {
        let &i = self.shown.get(self.selected)?;
        Some(self.items[i].action)
    }

    /// The query changed: only the lines that match, and the first of them chosen.
    fn filter(&mut self) {
        self.shown = (0..self.items.len())
            .filter(|&i| matches(&self.query, self.items[i].label))
            .collect();
        self.selected = 0;
    }
}

/// Whether `label` has the letters of `query` in the same order, not necessarily
/// next to each other, in any case: "ntb" matches "**N**ew **t**a**b**".
fn matches(query: &str, label: &str) -> bool {
    let mut label = label.chars().flat_map(char::to_lowercase);
    query
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| !c.is_whitespace())
        .all(|wanted| label.any(|c| c == wanted))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<Item> {
        let item = |label, action| Item {
            label,
            hint: "",
            action,
        };
        vec![
            item("New tab", Action::NewTab),
            item("Close tab", Action::CloseTab),
            item("Zoom in", Action::ZoomIn),
            item("Zoom out", Action::ZoomOut),
        ]
    }

    fn labels(palette: &Palette) -> Vec<&str> {
        palette.shown().map(|item| item.label).collect()
    }

    #[test]
    fn letters_in_order_not_next_to_each_other() {
        assert!(matches("ntb", "New tab"));
        assert!(matches("ZOOM", "Zoom in"));
        assert!(matches("zoom in", "Zoom in")); // spaces don't count
        assert!(matches("", "anything"));
        assert!(!matches("bt", "New tab")); // the order matters
        assert!(!matches("tabs", "New tab"));
    }

    #[test]
    fn typing_narrows_the_list() {
        let mut palette = Palette::new(items());
        assert_eq!(labels(&palette).len(), 4);
        palette.type_text("zo");
        assert_eq!(labels(&palette), ["Zoom in", "Zoom out"]);
        palette.type_text("o");
        palette.type_text("u");
        assert_eq!(labels(&palette), ["Zoom out"]);
        palette.backspace();
        assert_eq!(labels(&palette), ["Zoom in", "Zoom out"]);
    }

    #[test]
    fn arrows_go_round() {
        let mut palette = Palette::new(items());
        assert_eq!(palette.chosen(), Some(Action::NewTab));
        palette.move_selection(-1); // up from the first: the last
        assert_eq!(palette.chosen(), Some(Action::ZoomOut));
        palette.move_selection(1);
        assert_eq!(palette.chosen(), Some(Action::NewTab));
    }

    #[test]
    fn a_new_query_chooses_the_first_line_again() {
        let mut palette = Palette::new(items());
        palette.move_selection(2);
        palette.type_text("tab");
        assert_eq!(palette.chosen(), Some(Action::NewTab));
    }

    #[test]
    fn nothing_matches_nothing_to_do() {
        let mut palette = Palette::new(items());
        palette.type_text("xyz");
        assert_eq!(palette.shown().count(), 0);
        assert_eq!(palette.chosen(), None);
        palette.move_selection(1); // no panic on an empty list
    }
}
