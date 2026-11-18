//! The settings panel (Ctrl+,): the settings you choose among a few values or
//! set a number in, and whose change you see. ←/→ change the chosen one, and
//! Kel shows it at once; Enter keeps the changes (into `config.toml`), Esc
//! takes them all back.
//!
//! ```text
//! ┌ Settings ───────────────────── 1 changed ┐
//! │ LOOK                                     │
//! │ Glass opacity  1 solid, 0 glass ‹ 0.65 › │  the chosen one: ←/→
//! │ Grain                               0.10 │
//! │ ...                                      │
//! │ Open config.toml                  Ctrl+, │
//! ├──────────────────────────────────────────┤
//! │ ↑↓ choose  ←→ change  Enter keep  Esc    │
//! └──────────────────────────────────────────┘
//! ```
//!
//! The file stays what Kel reads: the panel doesn't keep settings of its own.
//! A change is shown by making the file's text with it (`config::with_settings`)
//! and giving Kel the config read from that, as a saved file would be; Enter
//! writes the same changes into the file. Text and lists (the shell, colors,
//! `solid_for`, `[ai]`) stay in the file: the last line opens it.
//!
//! Only the panel's state lives here: `App` sends it the keys and applies its
//! config, `renderer` draws it.

use crate::config::{self, Change, Config};
use toml::{Table, Value};

/// What a setting can be set to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A number from `min` to `max`, a `step` for each ←/→.
    Number { min: f64, max: f64, step: f64 },
    /// One of these words; after the last, the first again.
    Choice(&'static [&'static str]),
    /// true or false, shown as on and off.
    Switch,
    /// The terminal's font: one of the installed ones a terminal can use (see
    /// `font::monospace_families`), the list found when it's first changed.
    Font,
}

/// A setting the panel has.
#[derive(Debug, PartialEq)]
pub struct Setting {
    /// Its name in the config, with its section's: `"glass.opacity"`.
    pub name: &'static str,
    /// What the panel calls it: "Glass opacity".
    pub label: &'static str,
    /// A few words about it, shown on its line while it's chosen.
    pub note: &'static str,
    pub kind: Kind,
}

const fn number(
    name: &'static str,
    label: &'static str,
    note: &'static str,
    range: (f64, f64, f64),
) -> Setting {
    let (min, max, step) = range;
    let kind = Kind::Number { min, max, step };
    Setting {
        name,
        label,
        note,
        kind,
    }
}

const fn choice(
    name: &'static str,
    label: &'static str,
    note: &'static str,
    words: &'static [&'static str],
) -> Setting {
    Setting {
        name,
        label,
        note,
        kind: Kind::Choice(words),
    }
}

const fn switch(name: &'static str, label: &'static str, note: &'static str) -> Setting {
    Setting {
        name,
        label,
        note,
        kind: Kind::Switch,
    }
}

/// The panel's settings, in parts. A setting comes in when it fits the rule
/// above; its values are the config's (see the comments in `config.toml`).
pub const PARTS: &[(&str, &[Setting])] = &[
    (
        "LOOK",
        &[
            number(
                "glass.opacity",
                "Glass opacity",
                "1 solid, 0 all glass",
                (0.0, 1.0, 0.05),
            ),
            number("glass.grain", "Grain", "frosted glass", (0.0, 1.0, 0.05)),
            choice(
                "glass.effect",
                "Glass effect",
                "after a restart",
                &["acrylic", "mica", "none"],
            ),
            number(
                "layout.corner_radius",
                "Corners",
                "0 square, 8 soft",
                (0.0, 16.0, 1.0),
            ),
            choice(
                "layout.tab_bar",
                "Tab bar",
                "",
                &["always", "title", "hover", "never"],
            ),
            switch(
                "start_screen",
                "Start screen",
                "home's logo, recent projects",
            ),
            choice(
                "backdrop",
                "Backdrop",
                "the start screen's picture",
                &[
                    "auto",
                    "terrain",
                    "contours",
                    "mountains",
                    "sea",
                    "wind",
                    "stars",
                    "none",
                ],
            ),
        ],
    ),
    (
        "TEXT",
        &[
            Setting {
                name: "font",
                label: "Font",
                note: "",
                kind: Kind::Font,
            },
            number("font_size", "Font size", "", (8.0, 40.0, 1.0)),
            number(
                "line_height",
                "Line height",
                "1.2: a fifth more",
                (0.8, 2.0, 0.05),
            ),
            number("ui_scale", "UI scale", "tabs, menus", (0.5, 3.0, 0.05)),
            choice(
                "cursor",
                "Cursor",
                "programs can pick theirs",
                &["block", "underline", "bar"],
            ),
            switch("cursor_blink", "Cursor blink", ""),
        ],
    ),
    (
        "BEHAVIOR",
        &[
            choice(
                "bell",
                "Bell",
                "a program's \"done\"",
                &["taskbar", "flash", "none"],
            ),
            switch("copy_on_select", "Copy on select", "copied as you let go"),
        ],
    ),
];

/// A line of the panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Line {
    /// A part's name: "LOOK".
    Title(&'static str),
    Setting(&'static Setting),
    /// The last line: opens `config.toml`, for the rest.
    OpenFile,
}

pub struct Settings {
    /// The file's text as the panel opened: the changes are shown on it.
    text: String,
    /// That text read as a table, for a setting's value before a change; and
    /// the file Kel writes, for one the file doesn't have.
    file: Table,
    defaults: Table,
    /// The config Kel had as the panel opened: Esc goes back to it.
    before: Config,
    /// The settings changed, and their values now. A setting changed back to
    /// what it was isn't here.
    changes: Vec<Change>,
    lines: Vec<Line>,
    /// The chosen line: never a title.
    selected: usize,
    /// The installed fonts the font goes through, once they're found (see
    /// `needs_fonts`).
    fonts: Option<Vec<String>>,
}

impl Settings {
    /// The panel, over the config file as it is now and `config`, the one Kel
    /// has. An error if Kel can't read the file: then there's nothing to show
    /// changes on (it's to be fixed in the file).
    pub fn new(text: String, config: &Config) -> anyhow::Result<Settings> {
        config::with_settings(&text, &[])?;
        let mut lines = Vec::new();
        for (title, settings) in PARTS {
            lines.push(Line::Title(title));
            lines.extend(settings.iter().map(Line::Setting));
        }
        lines.push(Line::OpenFile);
        Ok(Settings {
            file: toml::from_str(&text)?,
            defaults: toml::from_str(config::DEFAULT_CONFIG)?,
            text,
            before: config.clone(),
            changes: Vec::new(),
            lines,
            selected: 1, // the first setting, under its part's name
            fonts: None,
        })
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn chosen(&self) -> Line {
        self.lines[self.selected]
    }

    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// The config Kel had as the panel opened.
    pub fn before(&self) -> &Config {
        &self.before
    }

    /// ↑ (-1) or ↓ (1): the line before or after, past the titles. Not past
    /// the first or the last.
    pub fn move_selection(&mut self, step: isize) {
        let mut at = self.selected;
        while let Some(next) = at
            .checked_add_signed(step)
            .filter(|&i| i < self.lines.len())
        {
            at = next;
            if !matches!(self.lines[at], Line::Title(_)) {
                self.selected = at;
                return;
            }
        }
    }

    /// `setting`'s value now: changed, as the file has it, or its default.
    pub fn value(&self, setting: &Setting) -> Value {
        self.changes
            .iter()
            .find(|(name, _)| *name == setting.name)
            .map(|(_, value)| value.clone())
            .or_else(|| self.saved(setting).cloned())
            .unwrap_or(Value::Boolean(false))
    }

    /// Whether `setting` was changed here.
    pub fn is_changed(&self, setting: &Setting) -> bool {
        self.changes.iter().any(|(name, _)| *name == setting.name)
    }

    /// Its value in the file, or its default.
    fn saved(&self, setting: &Setting) -> Option<&Value> {
        lookup(&self.file, setting.name).or_else(|| lookup(&self.defaults, setting.name))
    }

    /// Whether the chosen line is the font, and the installed fonts aren't
    /// found yet: `App` finds them then (`set_fonts`), as ←/→ come for the first
    /// time. Not as the panel opens: after a reboot it takes a second.
    pub fn needs_fonts(&self) -> bool {
        self.chosen() == Line::Setting(FONT) && self.fonts.is_none()
    }

    /// The installed fonts, the built-in one first. The font Kel has is among
    /// them even if it's none of these (one that doesn't say it's monospace):
    /// ←/→ come back to it.
    pub fn set_fonts(&mut self, mut fonts: Vec<String>) {
        if let Some(now) = self.value(FONT).as_str()
            && !fonts.iter().any(|f| f.eq_ignore_ascii_case(now))
        {
            let lower = now.to_lowercase();
            let at = fonts[1..].partition_point(|f| f.to_lowercase() < lower);
            fonts.insert(at + 1, now.to_string());
        }
        self.fonts = Some(fonts);
    }

    /// A few words about `setting`: the font, which of how many it is.
    pub fn note(&self, setting: &Setting) -> String {
        if setting.kind != Kind::Font {
            return setting.note.to_string();
        }
        let Some(fonts) = &self.fonts else {
            return "← → the installed ones".into();
        };
        match position(fonts, &self.value(setting)) {
            Some(at) => format!("{} of {}", at + 1, fonts.len()),
            None => String::new(),
        }
    }

    /// ← (-1) or → (1) on the chosen setting: its next value. `false` if
    /// nothing changed: the chosen line isn't a setting, or a number is at its
    /// end, or the fonts aren't found yet.
    pub fn change(&mut self, step: isize) -> bool {
        let Line::Setting(setting) = self.chosen() else {
            return false;
        };
        let now = self.value(setting);
        let next = match (setting.kind, &self.fonts) {
            (Kind::Font, Some(fonts)) => next_word(fonts, &now, step),
            (Kind::Font, None) => return false,
            (kind, _) => next_value(kind, &now, step),
        };
        if same(&next, &now) {
            return false;
        }
        self.changes.retain(|(name, _)| *name != setting.name);
        // Back to what the file has: no change.
        if !self.saved(setting).is_some_and(|saved| same(saved, &next)) {
            self.changes.push((setting.name, next));
        }
        true
    }

    /// The config with the changes: what Kel shows while the panel is open.
    pub fn preview(&self) -> anyhow::Result<Config> {
        Ok(config::with_settings(&self.text, &self.changes)?.1)
    }
}

/// The font's line, as `PARTS` has it.
const FONT: &Setting = &PARTS[1].1[0];

/// Where `now` is among `words`, in any case: `"consolas"` is `"Consolas"`.
fn position<S: AsRef<str>>(words: &[S], now: &Value) -> Option<usize> {
    let now = now.as_str()?;
    words
        .iter()
        .position(|w| w.as_ref().eq_ignore_ascii_case(now))
}

/// The word after `now` among `words` (before it, `step` -1); after the last,
/// the first. One that isn't among them: the first.
fn next_word<S: AsRef<str>>(words: &[S], now: &Value, step: isize) -> Value {
    let at = position(words, now).map_or(0, |at| {
        (at as isize + step).rem_euclid(words.len() as isize) as usize
    });
    Value::String(words[at].as_ref().to_string())
}

/// The value at `name` ("glass.opacity") in a config's table.
fn lookup<'a>(table: &'a Table, name: &str) -> Option<&'a Value> {
    let (section, key) = name.rsplit_once('.').unwrap_or(("", name));
    let mut table = table;
    for part in section.split('.').filter(|part| !part.is_empty()) {
        table = table.get(part)?.as_table()?;
    }
    table.get(key)
}

/// A number, written either way: `18` or `18.0`.
fn as_number(value: &Value) -> Option<f64> {
    value.as_float().or(value.as_integer().map(|i| i as f64))
}

/// The same value: 18 is 18.0.
fn same(a: &Value, b: &Value) -> bool {
    match (as_number(a), as_number(b)) {
        (Some(a), Some(b)) => (a - b).abs() < 1e-9,
        _ => a == b,
    }
}

/// The value a setting of `kind` has after `now`, a `step` on: -1 or 1.
fn next_value(kind: Kind, now: &Value, step: isize) -> Value {
    match kind {
        Kind::Number { min, max, step: by } => {
            let now = as_number(now).unwrap_or(min);
            // Onto the steps: 0.83 → 0.85 and 0.80, not 0.88 and 0.78. Numbers
            // with a point are a bit off (0.8 / 0.05 is 15.999999999999998, 0.1 +
            // 0.05 is 0.15000000000000002): rounded to 6 places first.
            let round = |x: f64| (x * 1e6).round() / 1e6;
            let at = round(now / by);
            let steps = if step > 0 {
                at.floor() + 1.0
            } else {
                at.ceil() - 1.0
            };
            let next = round((steps * by).clamp(min, max));
            Value::Float(next)
        }
        Kind::Choice(words) => next_word(words, now, step),
        Kind::Switch => Value::Boolean(!now.as_bool().unwrap_or(false)),
        Kind::Font => now.clone(), // its words are the panel's: see `change`
    }
}

/// How the panel shows a value: a number with as many decimals as its step
/// (`0.80` by 0.05, `18` by 1), a word as it is, a switch on or off.
pub fn show(kind: Kind, value: &Value) -> String {
    match kind {
        Kind::Number { step, .. } => {
            let x = as_number(value).unwrap_or(0.0);
            let decimals = if step.fract() == 0.0 { 0 } else { 2 };
            format!("{x:.decimals$}")
        }
        Kind::Choice(_) | Kind::Font => value.as_str().unwrap_or("").to_string(),
        Kind::Switch => if value.as_bool() == Some(true) {
            "on"
        } else {
            "off"
        }
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> Settings {
        Settings::new(config::DEFAULT_CONFIG.to_string(), &Config::default()).unwrap()
    }

    /// The panel's setting `name`.
    fn by_name(name: &str) -> &'static Setting {
        PARTS
            .iter()
            .flat_map(|(_, settings)| settings.iter())
            .find(|setting| setting.name == name)
            .unwrap()
    }

    /// Chooses the setting `name`.
    fn choose(panel: &mut Settings, name: &str) {
        panel.selected = panel
            .lines
            .iter()
            .position(|line| matches!(line, Line::Setting(s) if s.name == name))
            .unwrap();
    }

    #[test]
    fn every_setting_reads_its_default() {
        // Its name is in the file Kel writes, and its value is one the panel knows.
        let panel = panel();
        for (_, settings) in PARTS {
            for setting in *settings {
                let value = lookup(&panel.defaults, setting.name)
                    .unwrap_or_else(|| panic!("{} isn't in the default file", setting.name));
                match setting.kind {
                    Kind::Number { min, max, .. } => {
                        let x = as_number(value).unwrap();
                        assert!((min..=max).contains(&x), "{}", setting.name);
                    }
                    Kind::Choice(words) => {
                        assert!(words.contains(&value.as_str().unwrap()), "{}", setting.name)
                    }
                    Kind::Switch => assert!(value.is_bool(), "{}", setting.name),
                    Kind::Font => assert_eq!(value.as_str(), Some(crate::font::BUILT_IN)),
                }
            }
        }
    }

    #[test]
    fn every_value_kel_can_read() {
        // Each setting through all its values, one by one: Kel reads each.
        let mut panel = panel();
        panel.set_fonts(vec!["JetBrains Mono".into(), "Consolas".into()]);
        for (_, settings) in PARTS {
            for setting in *settings {
                choose(&mut panel, setting.name);
                for _ in 0..12 {
                    panel.change(1);
                    panel
                        .preview()
                        .unwrap_or_else(|e| panic!("{}: {e}", setting.name));
                }
            }
        }
    }

    #[test]
    fn a_number_by_its_steps() {
        let mut panel = panel();
        choose(&mut panel, "glass.opacity"); // 0.8
        assert!(panel.change(-1));
        assert_eq!(panel.changes(), [("glass.opacity", Value::Float(0.75))]);
        assert_eq!(panel.preview().unwrap().glass.opacity, 0.75);
        // Back to the file's: no change left.
        assert!(panel.change(1));
        assert!(panel.changes().is_empty());
        // Not past its end: 1.
        for _ in 0..10 {
            panel.change(1);
        }
        assert_eq!(panel.value(&PARTS[0].1[0]), Value::Float(1.0));
        assert!(!panel.change(1));
    }

    #[test]
    fn off_the_steps_comes_onto_them() {
        let range = Kind::Number {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        };
        assert_eq!(
            next_value(range, &Value::Float(0.83), 1),
            Value::Float(0.85)
        );
        assert_eq!(
            next_value(range, &Value::Float(0.83), -1),
            Value::Float(0.8)
        );
        assert_eq!(next_value(range, &Value::Float(0.1), 1), Value::Float(0.15));
        // A whole number in the file: 19.
        let size = Kind::Number {
            min: 8.0,
            max: 40.0,
            step: 1.0,
        };
        assert_eq!(next_value(size, &Value::Integer(19), 1), Value::Float(20.0));
    }

    #[test]
    fn words_go_round() {
        let mut panel = panel();
        choose(&mut panel, "cursor"); // block
        panel.change(-1);
        assert_eq!(panel.value(by_name("cursor")), Value::String("bar".into()));
        panel.change(1);
        assert!(panel.changes().is_empty());
        choose(&mut panel, "cursor_blink"); // on
        panel.change(1);
        assert!(!panel.preview().unwrap().cursor_blink);
    }

    #[test]
    fn up_and_down_past_the_titles() {
        let mut panel = panel();
        assert_eq!(panel.chosen(), Line::Setting(&PARTS[0].1[0]));
        panel.move_selection(-1); // the first: stays
        assert_eq!(panel.selected(), 1);
        choose(&mut panel, "backdrop"); // LOOK's last
        panel.move_selection(1);
        assert_eq!(panel.chosen(), Line::Setting(&PARTS[1].1[0])); // over "TEXT"
        panel.move_selection(-1);
        assert_eq!(panel.chosen(), Line::Setting(&PARTS[0].1[6]));
        for _ in 0..30 {
            panel.move_selection(1);
        }
        assert_eq!(panel.chosen(), Line::OpenFile);
    }

    #[test]
    fn the_files_values_not_the_defaults() {
        let text = "font_size = 19\n\n[glass]\nopacity = 0.6\n".to_string();
        let mut panel = Settings::new(text, &Config::default()).unwrap();
        assert_eq!(
            show(PARTS[0].1[0].kind, &panel.value(&PARTS[0].1[0])),
            "0.60"
        );
        assert_eq!(
            show(PARTS[0].1[1].kind, &panel.value(&PARTS[0].1[1])),
            "0.10"
        ); // its default
        choose(&mut panel, "font_size");
        let size = by_name("font_size");
        assert_eq!(show(size.kind, &panel.value(size)), "19");
        panel.change(1);
        assert_eq!(panel.preview().unwrap().font_size, 20.0);
    }

    #[test]
    fn fonts_go_round() {
        let mut panel = panel();
        choose(&mut panel, "font");
        // Not found yet: nothing to go through.
        assert!(panel.needs_fonts());
        assert!(!panel.change(1));
        let fonts = ["JetBrains Mono", "Consolas", "Courier New"];
        panel.set_fonts(fonts.map(String::from).to_vec());
        assert!(!panel.needs_fonts());
        assert_eq!(panel.note(FONT), "1 of 3");
        panel.change(1);
        assert_eq!(panel.preview().unwrap().font, "Consolas");
        assert_eq!(panel.note(FONT), "2 of 3");
        panel.change(-1);
        panel.change(-1); // round: the last
        assert_eq!(panel.value(FONT), Value::String("Courier New".into()));
    }

    #[test]
    fn the_font_kel_has_is_among_them() {
        // In the file in its own case, and one that isn't in the list (it
        // doesn't say it's monospace): in its place in the order.
        let text = "font = \"consolas\"\n".to_string();
        let mut panel = Settings::new(text, &Config::default()).unwrap();
        let fonts = ["JetBrains Mono", "Consolas", "Courier New"];
        panel.set_fonts(fonts.map(String::from).to_vec());
        assert_eq!(panel.fonts.as_ref().unwrap().len(), 3);
        assert_eq!(panel.note(FONT), "2 of 3");

        let text = "font = \"Iosevka\"\n".to_string();
        let mut panel = Settings::new(text, &Config::default()).unwrap();
        panel.set_fonts(fonts.map(String::from).to_vec());
        let names = ["JetBrains Mono", "Consolas", "Courier New", "Iosevka"];
        assert_eq!(panel.fonts.unwrap(), names);
    }

    #[test]
    fn a_file_kel_cant_read_has_no_panel() {
        assert!(Settings::new("font_size = \"big\"".into(), &Config::default()).is_err());
    }

    #[test]
    fn shown_values() {
        let opacity = Kind::Number {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        };
        assert_eq!(show(opacity, &Value::Float(0.8)), "0.80");
        assert_eq!(show(opacity, &Value::Integer(1)), "1.00");
        assert_eq!(show(Kind::Switch, &Value::Boolean(true)), "on");
        assert_eq!(show(Kind::Choice(&["a"]), &Value::String("a".into())), "a");
    }
}
