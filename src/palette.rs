//! The command palette (Ctrl+Shift+P): a search box over the terminal. Type a few
//! letters, the list narrows down, Enter does the chosen thing.
//!
//! Only its state lives here: what's typed, what's shown, which line is chosen.
//! `App` sends it the keys, `renderer` draws it on top of everything else.

use crate::keys::Action;

/// One line of the list. Its text is a `String`: a project's name is only
/// known when Kel runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// What it does, e.g. "New tab". What the search looks in.
    pub label: String,
    /// Shown dimmer at the right end, e.g. the shortcut "Ctrl+Shift+T".
    pub hint: String,
    pub action: Action,
}

/// Makes a line from what's typed, e.g. "Save the tabs as project: myapp" from
/// "myapp". `None`: no such line for this text.
type MakeItem = Box<dyn Fn(&str) -> Option<Item>>;

pub struct Palette {
    /// What's typed in the search box.
    query: String,
    /// Every line, in the order they're listed.
    items: Vec<Item>,
    /// The lines that match the query: their places in `items`.
    shown: Vec<usize>,
    /// The chosen line: its place in `shown`, or right after them: `typed`.
    selected: usize,
    /// Makes the line from what's typed (see `with_typed_item`).
    make_typed: Option<MakeItem>,
    /// That line for the query now, shown last.
    typed: Option<Item>,
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
            make_typed: None,
            typed: None,
        }
    }

    /// One more line, made by `make` from what's typed, after the ones that
    /// match: the list can't know a new project's name before it's typed.
    pub fn with_typed_item(mut self, make: impl Fn(&str) -> Option<Item> + 'static) -> Palette {
        self.make_typed = Some(Box::new(make));
        self
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// The lines that match, in order, and last the one made from what's typed.
    pub fn shown(&self) -> impl Iterator<Item = &Item> {
        let matching = self.shown.iter().map(|&i| &self.items[i]);
        matching.chain(&self.typed)
    }

    /// How many lines are shown.
    fn len(&self) -> usize {
        self.shown.len() + self.typed.is_some() as usize
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
        let n = self.len() as isize;
        if n > 0 {
            self.selected = (self.selected as isize + step).rem_euclid(n) as usize;
        }
    }

    /// What Enter would do now. `None` when nothing matches.
    pub fn chosen(&self) -> Option<Action> {
        let item = self.shown().nth(self.selected)?;
        Some(item.action.clone())
    }

    /// The query changed: only the lines that match, and the first of them chosen.
    fn filter(&mut self) {
        self.shown = (0..self.items.len())
            .filter(|&i| matches(&self.query, &self.items[i].label))
            .collect();
        self.typed = match &self.make_typed {
            Some(make) if !self.query.trim().is_empty() => make(&self.query),
            _ => None,
        };
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
        let item = |label: &str, action| Item {
            label: label.into(),
            hint: String::new(),
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
        palette.shown().map(|item| item.label.as_str()).collect()
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
    fn a_line_from_what_is_typed() {
        let make = |query: &str| {
            Some(Item {
                label: format!("Save as {query}"),
                hint: String::new(),
                action: Action::SaveProject(query.to_string()),
            })
        };
        let mut palette = Palette::new(items()).with_typed_item(make);
        // Nothing typed: no such line.
        assert_eq!(palette.shown().count(), 4);
        palette.type_text("zo");
        assert_eq!(labels(&palette), ["Zoom in", "Zoom out", "Save as zo"]);
        // The matching lines come first: Enter doesn't make a project by itself.
        assert_eq!(palette.chosen(), Some(Action::ZoomIn));
        palette.move_selection(-1);
        assert_eq!(palette.chosen(), Some(Action::SaveProject("zo".into())));
        // Nothing else matches: it's the only line, chosen.
        palette.type_text("xyz");
        assert_eq!(labels(&palette), ["Save as zoxyz"]);
        assert_eq!(palette.chosen(), Some(Action::SaveProject("zoxyz".into())));
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
