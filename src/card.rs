//! The card: a small box next to what it changes, for what was only in the
//! project's file before. A tab's name and command (double-click the tab):
//!
//! ```text
//!  ▶ server ×  >_ powershell
//!  ┌──────────────────────────────────┐
//!  │ Name     server█                 │
//!  │ Command  npm run dev             │
//!  │ Tab next   Enter saves   Esc     │
//!  └──────────────────────────────────┘
//! ```
//!
//! Only its state and where its fields are live here: `App` sends the keys and
//! the clicks and saves what's typed, `renderer` draws it.

use crate::layout::{Rect, scaled};
use crate::tab::TabId;

/// A line of text being typed, with a cursor in it (counted in letters).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextField {
    text: String,
    cursor: usize,
}

impl TextField {
    /// `text`, with the cursor at its end: that's where you go on typing.
    pub fn new(text: &str) -> TextField {
        TextField {
            text: text.to_string(),
            cursor: text.chars().count(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Where letter `n` starts in the string's bytes.
    fn byte(&self, n: usize) -> usize {
        self.text
            .char_indices()
            .nth(n)
            .map_or(self.text.len(), |(i, _)| i)
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// Types `text` at the cursor. A line break (in pasted text) is a space: a
    /// name or a command is one line.
    pub fn insert(&mut self, text: &str) {
        let text: String = text
            .chars()
            .map(|c| if c == '\n' { ' ' } else { c })
            .filter(|c| !c.is_control())
            .collect();
        let at = self.byte(self.cursor);
        self.text.insert_str(at, &text);
        self.cursor += text.chars().count();
    }

    /// Backspace: the letter before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let at = self.byte(self.cursor);
            self.text.remove(at);
        }
    }

    /// Ctrl+Backspace: the word before the cursor, and the spaces after it.
    pub fn backspace_word(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        let mut start = self.cursor;
        while start > 0 && chars[start - 1] == ' ' {
            start -= 1;
        }
        while start > 0 && chars[start - 1] != ' ' {
            start -= 1;
        }
        let (from, to) = (self.byte(start), self.byte(self.cursor));
        self.text.replace_range(from..to, "");
        self.cursor = start;
    }

    /// Delete: the letter after the cursor.
    pub fn delete(&mut self) {
        if self.cursor < self.len() {
            let at = self.byte(self.cursor);
            self.text.remove(at);
        }
    }

    /// Left (-1) or right (1), not past the ends.
    pub fn move_cursor(&mut self, step: isize) {
        self.cursor = self.cursor.saturating_add_signed(step).min(self.len());
    }

    /// Home (`false`) or End (`true`).
    pub fn jump(&mut self, to_end: bool) {
        self.cursor = if to_end { self.len() } else { 0 };
    }

    /// The part of the text that fits in `chars` letters, around the cursor, and
    /// where the cursor is in it. A long command scrolls: the cursor stays in sight.
    pub fn visible(&self, chars: usize) -> (String, usize) {
        let chars = chars.max(1);
        // Room for the cursor after the last letter too.
        let first = (self.cursor + 1).saturating_sub(chars);
        let shown = self.text.chars().skip(first).take(chars).collect();
        (shown, self.cursor - first)
    }

    /// What was typed, without spaces at the ends. Nothing typed: `None`.
    pub fn value(&self) -> Option<String> {
        let text = self.text.trim();
        (!text.is_empty()).then(|| text.to_string())
    }
}

/// What a card changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Tab(TabId),
}

/// One line of the card: what it is, and what's typed in it.
pub struct Field {
    pub label: &'static str,
    /// Shown dim while it's empty: what empty means.
    pub empty: &'static str,
    /// Under the fields while it has the cursor: what it does.
    pub note: &'static str,
    pub text: TextField,
}

pub struct Card {
    pub target: Target,
    fields: Vec<Field>,
    /// The field with the cursor.
    focus: usize,
}

impl Card {
    /// The card of tab `id`: its name, and in a project its command (home has no
    /// file to keep one in).
    pub fn tab(id: TabId, name: Option<&str>, command: Option<&str>, in_project: bool) -> Card {
        let mut fields = vec![Field {
            label: "Name",
            empty: "the program's title",
            note: "Empty: the program's title",
            text: TextField::new(name.unwrap_or("")),
        }];
        if in_project {
            fields.push(Field {
                label: "Command",
                empty: "none",
                note: "Runs when the project opens",
                text: TextField::new(command.unwrap_or("")),
            });
        }
        Card {
            target: Target::Tab(id),
            fields,
            focus: 0,
        }
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    /// The field with the cursor, for typing.
    pub fn focused(&mut self) -> &mut TextField {
        &mut self.fields[self.focus].text
    }

    /// Tab (1) or Shift+Tab (-1): the next field, round from the last to the first.
    pub fn next_field(&mut self, step: isize) {
        let n = self.fields.len() as isize;
        self.focus = (self.focus as isize + step).rem_euclid(n) as usize;
    }

    /// A click on field `i`: the cursor goes there.
    pub fn focus_field(&mut self, i: usize) {
        if i < self.fields.len() {
            self.focus = i;
        }
    }

    /// What's typed in the field `label`: `None` if it's empty, or not on this card.
    pub fn value(&self, label: &str) -> Option<String> {
        let field = self.fields.iter().find(|f| f.label == label)?;
        field.text.value()
    }

    /// Whether the card has the field `label`.
    pub fn has(&self, label: &str) -> bool {
        self.fields.iter().any(|f| f.label == label)
    }

    /// The field at (x, y) in the window, with the card at `rect`.
    pub fn field_at(&self, rect: Rect, scale: f32, x: usize, y: usize) -> Option<usize> {
        (0..self.fields.len()).find(|&i| field_rect(rect, scale, i).contains(x, y))
    }
}

/// The card's size and the room in it, pixels at 100%: its width, the room around
/// the fields, a field's height, the line with the keys under them.
const WIDTH: usize = 380;
const PADDING: usize = 8;
const FIELD_HEIGHT: usize = 30;
const HINT_HEIGHT: usize = 28;

/// Where the card goes: under `anchor` (the tab), from its left edge, moved left
/// if it would go past the window's right edge. e.g. a tab at x = 700 in a
/// 1000-wide window: 1000 - 8 - 380 = 612.
pub fn rect(anchor: Rect, window: Rect, scale: f32, fields: usize) -> Rect {
    let s = |n: usize| scaled(n, scale);
    let w = s(WIDTH).min(window.w.saturating_sub(2 * s(PADDING)));
    let right = (window.x + window.w).saturating_sub(s(PADDING));
    Rect {
        x: anchor.x.min(right.saturating_sub(w)),
        y: anchor.y + anchor.h + s(4),
        w,
        h: 2 * s(PADDING) + fields * s(FIELD_HEIGHT) + s(HINT_HEIGHT),
    }
}

/// Where field `i` is in the card at `card`.
pub fn field_rect(card: Rect, scale: f32, i: usize) -> Rect {
    let s = |n: usize| scaled(n, scale);
    Rect {
        x: card.x + s(PADDING),
        y: card.y + s(PADDING) + i * s(FIELD_HEIGHT),
        w: card.w.saturating_sub(2 * s(PADDING)),
        h: s(FIELD_HEIGHT),
    }
}

/// Where the line with the keys goes, under `fields` fields.
pub fn hint_y(card: Rect, scale: f32, fields: usize) -> usize {
    field_rect(card, scale, fields).y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_in_a_field() {
        let mut f = TextField::new("npm run");
        f.insert(" dev");
        assert_eq!((f.text(), f.cursor()), ("npm run dev", 11));
        f.jump(false);
        f.insert("> ");
        assert_eq!(f.text(), "> npm run dev");
        f.backspace(); // the space before "npm"
        f.delete(); // the "n"
        assert_eq!((f.text(), f.cursor()), (">pm run dev", 1));
        f.move_cursor(-5); // not past the start
        assert_eq!(f.cursor(), 0);
        // Not ASCII: letters, not bytes.
        let mut f = TextField::new("тест");
        f.move_cursor(-1);
        f.backspace();
        assert_eq!(f.text(), "тет");
    }

    #[test]
    fn a_pasted_line_and_words() {
        let mut f = TextField::new("");
        f.insert("cargo\r\nrun\t");
        assert_eq!(f.text(), "cargo run"); // the line break is a space, the rest goes
        f.insert("  --release");
        f.backspace_word(); // "--release": no space in it, one word
        assert_eq!(f.text(), "cargo run  ");
        f.backspace_word(); // the spaces, and "run"
        assert_eq!((f.text(), f.cursor()), ("cargo ", 6));
        // Spaces at the ends don't count; nothing typed is no value.
        assert_eq!(TextField::new("  dev ").value(), Some("dev".into()));
        assert_eq!(TextField::new("   ").value(), None);
    }

    #[test]
    fn a_long_text_scrolls_with_the_cursor() {
        let f = TextField::new("abcdefghij");
        // 5 letters of room: the cursor after "j" takes one.
        assert_eq!(f.visible(5), ("ghij".to_string(), 4));
        let mut f = f;
        f.jump(false);
        assert_eq!(f.visible(5), ("abcde".to_string(), 0));
    }

    #[test]
    fn a_tab_s_card() {
        let mut card = Card::tab(3, Some("server"), Some("npm run dev"), true);
        assert_eq!(card.fields().len(), 2);
        card.next_field(1);
        card.focused().insert(" -- --port 4000");
        assert_eq!(
            card.value("Command"),
            Some("npm run dev -- --port 4000".into())
        );
        card.next_field(1); // round to the name
        card.focused().jump(false);
        card.focused().delete();
        assert_eq!(card.value("Name"), Some("erver".into()));
        // Home: no command.
        let card = Card::tab(1, None, None, false);
        assert!(!card.has("Command"));
        assert_eq!(card.value("Name"), None);
    }

    #[test]
    fn where_the_card_goes() {
        let window = Rect {
            x: 0,
            y: 0,
            w: 1000,
            h: 600,
        };
        let tab = Rect {
            x: 36,
            y: 8,
            w: 180,
            h: 32,
        };
        let card = rect(tab, window, 1.0, 2);
        assert_eq!((card.x, card.y, card.w, card.h), (36, 44, 380, 104));
        // A tab near the right edge: the card moves left to fit.
        let far = rect(Rect { x: 700, ..tab }, window, 1.0, 2);
        assert_eq!(far.x, 612);
        // Fields at 52..82 and 82..112.
        let card_ = Card::tab(1, None, None, true);
        assert_eq!(card_.field_at(card, 1.0, 100, 60), Some(0));
        assert_eq!(card_.field_at(card, 1.0, 100, 90), Some(1));
        assert_eq!(card_.field_at(card, 1.0, 100, 130), None); // the keys' line
    }
}
