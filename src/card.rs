//! The card: a small box next to what it changes, for what was only in the
//! project's file before. A project's name and color (F2 or a right click in the
//! sidebar), a tab's name and command (double-click the tab, or a right click):
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
use crate::theme;

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
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Tab(TabId),
    /// The project of this name: its name as it was when the card opened.
    Project(String),
}

/// One line of the card: what it is, and what's typed in it.
pub struct Field {
    pub label: &'static str,
    /// Shown dim while it's empty: what empty means.
    pub empty: &'static str,
    /// Under the fields while it has the cursor: what it does.
    pub note: &'static str,
    pub text: TextField,
    /// A color's field: the colors ←→ choose from, before its `#hex`. `None`:
    /// no color of its own (gray, or the theme's accent). Empty for a text field.
    pub swatches: Vec<Option<u32>>,
}

impl Field {
    fn text(label: &'static str, empty: &'static str, note: &'static str, text: &str) -> Field {
        Field {
            label,
            empty,
            note,
            text: TextField::new(text),
            swatches: Vec::new(),
        }
    }

    /// The color typed: `Some(None)` for none (empty), `None` if it isn't one
    /// (yet). The `#` can be left out.
    pub fn color(&self) -> Option<Option<u32>> {
        let text = self.text.text().trim();
        if text.is_empty() {
            return Some(None);
        }
        let hex = if text.starts_with('#') {
            text.to_string()
        } else {
            format!("#{text}")
        };
        theme::parse_hex(&hex).map(Some)
    }

    /// Which of the swatches is the color typed, if one is.
    pub fn swatch(&self) -> Option<usize> {
        let color = self.color()?;
        self.swatches.iter().position(|&s| s == color)
    }

    /// Types `text` at the cursor. A color's field takes only what a color can
    /// be: a `#` at its start and at most 6 hex digits after it. The rest isn't
    /// typed: e.g. "#e0af68ff" pasted is "#e0af68".
    fn type_text(&mut self, text: &str) {
        if self.swatches.is_empty() {
            return self.text.insert(text);
        }
        for c in text.trim().chars() {
            let now = self.text.text();
            let hash = now.starts_with('#');
            let digits = now.chars().filter(char::is_ascii_hexdigit).count();
            let at_start = self.text.cursor() == 0;
            let fits = match c {
                '#' => at_start && !hash,
                // Not before the `#`.
                c if c.is_ascii_hexdigit() => digits < 6 && !(at_start && hash),
                _ => false,
            };
            if fits {
                self.text.insert(c.encode_utf8(&mut [0; 4]));
            }
        }
    }
}

pub struct Card {
    pub target: Target,
    fields: Vec<Field>,
    /// The field with the cursor.
    focus: usize,
    /// Why what's typed can't be kept (a name taken, not a color): under the
    /// fields, in red, until the next key.
    pub error: Option<String>,
}

impl Card {
    /// The card of tab `id`: its name, and in a project its command (home has no
    /// file to keep one in).
    pub fn tab(id: TabId, name: Option<&str>, command: Option<&str>, in_project: bool) -> Card {
        let name = name.unwrap_or("");
        let mut fields = vec![Field::text(
            "Name",
            "the program's title",
            "Empty: the program's title",
            name,
        )];
        if in_project {
            let command = command.unwrap_or("");
            let note = "Runs when the project opens";
            fields.push(Field::text("Command", "none", note, command));
        }
        Card {
            target: Target::Tab(id),
            fields,
            focus: 0,
            error: None,
        }
    }

    /// The card of the project `name`: its name, and its color among `colors`
    /// (the theme's) or a `#hex` of its own.
    pub fn project(name: &str, color: Option<u32>, colors: &[u32]) -> Card {
        let hex = color.map_or(String::new(), |c| format!("#{c:06x}"));
        let mut swatches: Vec<Option<u32>> = colors.iter().map(|&c| Some(c)).collect();
        swatches.push(None);
        let color = Field {
            swatches,
            ..Field::text("Color", "none", "←→ a color, or type a #hex", &hex)
        };
        let note = "A new name renames its file";
        Card {
            target: Target::Project(name.to_string()),
            fields: vec![Field::text("Name", "a name", note, name), color],
            focus: 0,
            error: None,
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

    /// Typed or pasted `text`, into the field with the cursor (see `Field::type_text`).
    pub fn type_text(&mut self, text: &str) {
        self.fields[self.focus].type_text(text);
    }

    /// Tab (1) or Shift+Tab (-1): the next field, round from the last to the first.
    pub fn next_field(&mut self, step: isize) {
        let n = self.fields.len() as isize;
        self.focus = (self.focus as isize + step).rem_euclid(n) as usize;
    }

    /// ←→ in a color's field: the next swatch (or the first, from a color of
    /// its own), its `#hex` typed in. Returns `false` in a text field: there ←→
    /// move the cursor.
    pub fn step_swatch(&mut self, step: isize) -> bool {
        let field = &mut self.fields[self.focus];
        let n = field.swatches.len() as isize;
        if n == 0 {
            return false;
        }
        let next = match field.swatch() {
            Some(i) => (i as isize + step).rem_euclid(n),
            None if step > 0 => 0,
            None => n - 1,
        };
        let hex = field.swatches[next as usize].map_or(String::new(), |c| format!("#{c:06x}"));
        field.text = TextField::new(&hex);
        true
    }

    /// The color typed in the field "Color": `Err` if it isn't one.
    pub fn color(&self) -> Result<Option<u32>, String> {
        let field = self.fields.iter().find(|f| f.label == "Color");
        let Some(field) = field else {
            return Ok(None);
        };
        let text = field.text.text().trim();
        field
            .color()
            .ok_or_else(|| format!("\"{text}\" isn't a color like #ff8000"))
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
const WIDTH: usize = 420;
const PADDING: usize = 8;
const FIELD_HEIGHT: usize = 30;
const HINT_HEIGHT: usize = 28;

/// Where the card goes: under `anchor` (the tab), from its left edge, moved left
/// if it would go past the window's right edge. e.g. a tab at x = 700 in a
/// 1000-wide window: 1000 - 8 - 420 = 572.
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
    fn a_project_s_color() {
        let colors = [0xf7768e, 0x9ece6a, 0x7aa2f7];
        let mut card = Card::project("kel", Some(0x9ece6a), &colors);
        card.next_field(1); // the color
        assert_eq!(card.color(), Ok(Some(0x9ece6a)));
        assert!(card.step_swatch(1));
        assert_eq!(card.color(), Ok(Some(0x7aa2f7)));
        assert!(card.step_swatch(1)); // the last: none
        assert_eq!(card.color(), Ok(None));
        assert!(card.step_swatch(1)); // round to the first
        assert_eq!(card.color(), Ok(Some(0xf7768e)));
        // A color of its own, typed, with or without '#'.
        card.focused().jump(true);
        for _ in 0..7 {
            card.focused().backspace();
        }
        card.type_text("e0af68");
        assert_eq!(card.color(), Ok(Some(0xe0af68)));
        assert!(card.step_swatch(-1)); // not a swatch: from the end
        assert_eq!(card.color(), Ok(None));
        card.type_text("#12");
        assert!(card.color().is_err());
        // In the name ←→ are the cursor's.
        card.next_field(1);
        assert!(!card.step_swatch(1));
    }

    #[test]
    fn a_hex_is_at_most_6_digits() {
        let mut card = Card::project("kel", None, &[0x112233]);
        card.next_field(1); // the color, empty
        let typed = |card: &Card| card.value("Color").unwrap_or_default();
        card.type_text("#e0af68ff"); // a 7th and 8th: not typed
        assert_eq!(typed(&card), "#e0af68");
        card.type_text("1");
        assert_eq!(typed(&card), "#e0af68");
        // Only hex digits; one `#`, at the start.
        card.focused().backspace();
        card.type_text("xz#g");
        assert_eq!(typed(&card), "#e0af6");
        card.focused().jump(false); // to the start
        card.type_text("9#");
        assert_eq!(typed(&card), "#e0af6");
        // Without a `#` it's a color too; one can be added at the start.
        let mut card = Card::project("kel", None, &[]);
        card.next_field(1);
        card.type_text(" E0AF68 ");
        assert_eq!(typed(&card), "E0AF68");
        card.focused().jump(false);
        card.type_text("#");
        assert_eq!(card.color(), Ok(Some(0xe0af68)));
        // A name takes anything.
        card.next_field(1);
        card.type_text("my #1 project!");
        assert_eq!(card.value("Name").as_deref(), Some("kelmy #1 project!"));
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
        assert_eq!((card.x, card.y, card.w, card.h), (36, 44, 420, 104));
        // A tab near the right edge: the card moves left to fit.
        let far = rect(Rect { x: 700, ..tab }, window, 1.0, 2);
        assert_eq!(far.x, 572);
        // Fields at 52..82 and 82..112.
        let card_ = Card::tab(1, None, None, true);
        assert_eq!(card_.field_at(card, 1.0, 100, 60), Some(0));
        assert_eq!(card_.field_at(card, 1.0, 100, 90), Some(1));
        assert_eq!(card_.field_at(card, 1.0, 100, 130), None); // the keys' line
    }
}
