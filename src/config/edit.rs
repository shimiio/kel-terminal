//! Changes one setting in the config file, as you would by hand: that value, in
//! its line, and nothing else. The comments, the order, the empty lines stay.
//!
//! `glass.opacity` to 0.65:
//!
//! ```text
//! [glass]                                 [glass]
//! effect = "acrylic"                      effect = "acrylic"
//! opacity = 0.8    # how much of ...  →   opacity = 0.65   # how much of ...
//! ```
//!
//! Not in the file, a setting comes back from its example (`# margin = 0` →
//! `margin = 4`), or goes after the last setting of its section.
//!
//! The settings panel (Ctrl+,) writes with it. Kel notices the save as it notices
//! yours (`config::modified`), and applies it.

use super::{Config, DEFAULT_CONFIG, parse_toml};
use std::io::ErrorKind;

/// Writes `value` for `setting` into the config file. `setting` is its
/// section's name and its own, with dots: `"font_size"` (no section),
/// `"glass.opacity"`, `"layout.title.margin"`.
///
/// The file is written only if Kel can read what comes out: a wrong value
/// (`font_size = "big"`) is an error and the file stays as it was.
pub fn save_setting(setting: &str, value: toml::Value) -> anyhow::Result<()> {
    let path = super::path()?;
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        // No file yet: the one Kel writes on the first start, with the change.
        Err(e) if e.kind() == ErrorKind::NotFound => {
            std::fs::create_dir_all(path.parent().unwrap())?;
            DEFAULT_CONFIG.to_string()
        }
        Err(e) => return Err(e.into()),
    };
    std::fs::write(&path, checked(&text, setting, &value)?)?;
    Ok(())
}

/// `set`, if Kel can read what comes out.
fn checked(text: &str, setting: &str, value: &toml::Value) -> anyhow::Result<String> {
    let new = set(text, setting, value);
    parse_toml::<Config>("config.toml", &new)?;
    Ok(new)
}

/// `text` with `setting` set to `value` (see `save_setting`).
fn set(text: &str, setting: &str, value: &toml::Value) -> String {
    let (section, key) = setting.rsplit_once('.').unwrap_or(("", setting));
    let value = format_value(value);
    let found = find(text, section, key);

    // `lines()` takes away "\n" and "\r\n" both; the file gets back its own.
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    match found {
        Found::Setting { first, last } => {
            lines[first] = replace_value(&lines[first], &value);
            // A value of several lines (`palette = [` ... `]`) becomes one.
            lines.drain(first + 1..=last);
        }
        Found::Example(line) => lines[line] = replace_value(&lines[line], &value),
        Found::Section(after) => lines.insert(after, format!("{key} = {value}")),
        Found::Nothing => {
            if lines.last().is_some_and(|line| !line.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[{section}]"));
            lines.push(format!("{key} = {value}"));
        }
    }
    let mut new = lines.join(newline);
    new.push_str(newline);
    new
}

/// Where the setting is, or would go: places in `text.lines()`.
#[derive(Debug, PartialEq)]
enum Found {
    /// The setting, from its first line to its last (the same, unless its value
    /// has several lines).
    Setting { first: usize, last: usize },
    /// Not there, but an example of it is, commented out: `# margin = 0`.
    Example(usize),
    /// Neither, but its section is: it goes in at this place, after the
    /// section's last setting (its header, if it has none).
    Section(usize),
    /// Not even the section.
    Nothing,
}

fn find(text: &str, section: &str, key: &str) -> Found {
    let mut setting = None;
    let mut example = None;
    // The settings without a section are at the top, before any header: there,
    // a new one goes first, if there are none.
    let mut insert_at = section.is_empty().then_some(0);

    let mut current = ""; // the section of the line now
    let mut depth = 0; // brackets open from the lines before: inside a value
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let inside = depth > 0;
        depth += brackets(line);
        if inside {
            // A line of a value that began above: `"#5c5c5c", "#ff4d5e",`.
            if current == section && insert_at.is_some() {
                insert_at = Some(i + 1);
            }
            if let Some(Found::Setting { last, .. }) = &mut setting
                && *last == i - 1
            {
                *last = i;
            }
            continue;
        }
        if let Some(name) = header(trimmed) {
            depth = 0; // a header's brackets are its own
            current = name;
            if current == section {
                insert_at = Some(i + 1);
            }
            continue;
        }
        if current != section {
            continue;
        }
        if key_of(trimmed) == Some(key) {
            setting.get_or_insert(Found::Setting { first: i, last: i });
        }
        if key_of(trimmed).is_some() {
            insert_at = Some(i + 1);
        } else if let Some(body) = trimmed.strip_prefix('#')
            && key_of(body.trim_start_matches('#').trim_start()) == Some(key)
        {
            example.get_or_insert(i);
        }
    }
    setting
        .or(example.map(Found::Example))
        .or(insert_at.map(Found::Section))
        .unwrap_or(Found::Nothing)
}

/// The section a header line opens: `[layout.title]` → `layout.title`.
fn header(trimmed: &str) -> Option<&str> {
    let name = trimmed.strip_prefix('[')?;
    let name = name.strip_prefix('[').unwrap_or(name); // `[[tab]]`
    Some(name[..name.find(']')?].trim())
}

/// The setting a line sets: `opacity = 0.8  # how much` → `opacity`. `None`
/// for any other line, e.g. a comment's text: `Font size in pixels. Ctrl+= ...`
/// has spaces before its `=`, which a setting's name can't.
fn key_of(trimmed: &str) -> Option<&str> {
    let name = trimmed[..trimmed.find('=')?].trim_end();
    let bare = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    (!name.is_empty() && name.chars().all(bare)).then_some(name)
}

/// How many more brackets `line` opens than it closes: `palette = [` 1, `]` -1.
/// Not those in strings, or in the comment.
fn brackets(line: &str) -> i32 {
    let code = &line[..comment_start(line)];
    let mut depth = 0;
    let mut quote = None;
    let mut escaped = false;
    for c in code.chars() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' && q == '"' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ => {}
            },
        }
    }
    depth
}

/// Where the comment at the end of `line` begins (its `#`), or the line's end.
/// A `#` in a string isn't one: `solid_color = "#000000" # its color`.
fn comment_start(line: &str) -> usize {
    let mut quote = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' && q == '"' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' => return i,
            None => {}
        }
    }
    line.len()
}

/// `line` (`key = value  # comment`) with `value` in place of its own. A
/// commented-out example (`# key = ...`) loses its `#`. The comment after the
/// value stays in its column, if the new value leaves room:
///
/// ```text
/// opacity = 0.8    # how much       # accent = "#569cd6" # home's color
/// opacity = 0.65   # how much       accent = "#e06c75"   # home's color
/// ```
fn replace_value(line: &str, value: &str) -> String {
    let indent = &line[..line.len() - line.trim_start().len()];
    let mut body = line.trim_start();
    if body.starts_with('#') {
        body = body.trim_start_matches('#').trim_start();
    }
    // Up to the value: `opacity = `, as it's written.
    let equals = body.find('=').expect("a setting's line has its =") + 1;
    let start = equals + (body[equals..].len() - body[equals..].trim_start().len());
    let comment = &body[start + comment_start(&body[start..])..];

    let mut new = format!("{indent}{}{value}", &body[..start]);
    if !comment.is_empty() {
        let column = line[..line.len() - comment.len()].chars().count();
        let gap = column.saturating_sub(new.chars().count()).max(1);
        new.push_str(&" ".repeat(gap));
        new.push_str(comment);
    }
    new
}

/// A value as the config writes it: whole numbers without `.0` (`ui_scale = 1`,
/// as in the file Kel writes; Kel reads them as numbers with a point too).
fn format_value(value: &toml::Value) -> String {
    match value {
        toml::Value::Float(x) if x.fract() == 0.0 && x.abs() < 1e15 => format!("{}", *x as i64),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml::Value;

    fn float(x: f64) -> Value {
        Value::Float(x)
    }

    fn string(s: &str) -> Value {
        Value::String(s.to_string())
    }

    /// The lines of `before` and `after` that differ, side by side.
    fn changed<'a>(before: &'a str, after: &'a str) -> Vec<(&'a str, &'a str)> {
        assert_eq!(before.lines().count(), after.lines().count());
        before
            .lines()
            .zip(after.lines())
            .filter(|(a, b)| a != b)
            .collect()
    }

    #[test]
    fn value_changes_comment_stays_in_its_column() {
        let text = "[glass]\nopacity = 0.8    # how much\n";
        assert_eq!(
            set(text, "glass.opacity", &float(0.65)),
            "[glass]\nopacity = 0.65   # how much\n"
        );
        // Longer than the room: one space.
        let text = "[glass]\nopacity = 0.8 # how much\n";
        assert_eq!(
            set(text, "glass.opacity", &float(0.65)),
            "[glass]\nopacity = 0.65 # how much\n"
        );
    }

    #[test]
    fn only_that_line_changes() {
        let new = set(DEFAULT_CONFIG, "glass.opacity", &float(0.65));
        assert_eq!(
            changed(DEFAULT_CONFIG, &new),
            [(
                "opacity = 0.8    # how much of the theme's colors cover the glass: 1 solid, 0 all glass",
                "opacity = 0.65   # how much of the theme's colors cover the glass: 1 solid, 0 all glass"
            )]
        );
    }

    #[test]
    fn a_setting_by_its_whole_name() {
        // `cursor` isn't `cursor_blink`, which comes after it.
        let new = set(DEFAULT_CONFIG, "cursor", &string("bar"));
        assert_eq!(
            changed(DEFAULT_CONFIG, &new),
            [("cursor = \"block\"", "cursor = \"bar\"")]
        );
        let new = set(DEFAULT_CONFIG, "cursor_blink", &Value::Boolean(false));
        assert_eq!(
            changed(DEFAULT_CONFIG, &new),
            [(
                "cursor_blink = true   # false: it stands still (the logo's and the sidebar's too)",
                "cursor_blink = false  # false: it stands still (the logo's and the sidebar's too)"
            )]
        );
    }

    #[test]
    fn the_section_decides() {
        // `margin` is in [layout], and as an example in [layout.title].
        let new = set(DEFAULT_CONFIG, "layout.margin", &Value::Integer(4));
        assert_eq!(
            changed(DEFAULT_CONFIG, &new),
            [("margin = 8", "margin = 4")]
        );
        let new = set(DEFAULT_CONFIG, "layout.title.margin", &Value::Integer(0));
        assert_eq!(
            changed(DEFAULT_CONFIG, &new),
            [(
                "# margin = 0      # one sheet, edge to edge",
                "margin = 0        # one sheet, edge to edge"
            )]
        );
        let config: Config = toml::from_str(&new).unwrap();
        assert_eq!(config.layout.margin, 8);
    }

    #[test]
    fn an_example_comes_back() {
        // A `#` in a string isn't a comment; the next line's comment, which goes
        // on with this one's, stays under it.
        let new = set(DEFAULT_CONFIG, "colors.accent", &string("#e06c75"));
        assert_eq!(
            changed(DEFAULT_CONFIG, &new),
            [(
                "# accent = \"#569cd6\" # home's color, and of projects without their own: the tab",
                "accent = \"#e06c75\"   # home's color, and of projects without their own: the tab"
            )]
        );
        let config: Config = toml::from_str(&new).unwrap();
        assert_eq!(config.colors.accent, Some(0xe06c75));
    }

    #[test]
    fn a_missing_setting_goes_after_its_sections_last() {
        // Before the comments of the next section, which are about it.
        let text = "[glass]\neffect = \"acrylic\"\n\n# Colors.\n[colors]\nfrom = 1\n";
        assert_eq!(
            set(text, "glass.grain", &float(0.3)),
            "[glass]\neffect = \"acrylic\"\ngrain = 0.3\n\n# Colors.\n[colors]\nfrom = 1\n"
        );
        // Without a section: before the first header.
        assert_eq!(
            set(text, "font_size", &float(20.0)),
            format!("font_size = 20\n{text}")
        );
        // A section with nothing yet: right under its header.
        assert_eq!(
            set("[ai]\n# Ask.\n", "ai.check", &Value::Boolean(false)),
            "[ai]\ncheck = false\n# Ask.\n"
        );
    }

    #[test]
    fn a_value_of_several_lines() {
        // A missing setting goes after the `]`, not in the middle of the list; a
        // line starting with `[` in it is no header.
        let text =
            "[colors]\npalette = [\n  [1, 2],\n  \"#000000\", # black\n]\n\n[ai]\nmodel = \"x\"\n";
        assert_eq!(
            set(text, "colors.done", &string("#00ff00")),
            "[colors]\npalette = [\n  [1, 2],\n  \"#000000\", # black\n]\ndone = \"#00ff00\"\n\n[ai]\nmodel = \"x\"\n"
        );
        // That value set: one line.
        assert_eq!(
            set(text, "colors.palette", &Value::Array(vec![])),
            "[colors]\npalette = []\n\n[ai]\nmodel = \"x\"\n"
        );
    }

    #[test]
    fn a_missing_section_goes_last() {
        assert_eq!(
            set("font_size = 18\n", "glass.grain", &float(0.3)),
            "font_size = 18\n\n[glass]\ngrain = 0.3\n"
        );
        assert_eq!(set("", "font_size", &float(18.0)), "font_size = 18\n");
    }

    #[test]
    fn line_endings_stay() {
        assert_eq!(
            set("a = 1\r\nb = 2\r\n", "b", &float(3.0)),
            "a = 1\r\nb = 3\r\n"
        );
    }

    #[test]
    fn values_as_the_config_writes_them() {
        assert_eq!(format_value(&float(1.0)), "1");
        assert_eq!(format_value(&float(1.25)), "1.25");
        assert_eq!(format_value(&Value::Integer(10000)), "10000");
        assert_eq!(format_value(&Value::Boolean(true)), "true");
        assert_eq!(
            format_value(&string("JetBrains Mono")),
            "\"JetBrains Mono\""
        );
    }

    #[test]
    fn what_kel_cant_read_isnt_written() {
        let new = checked(DEFAULT_CONFIG, "ui_scale", &float(1.25)).unwrap();
        assert_eq!(toml::from_str::<Config>(&new).unwrap().ui_scale, 1.25);
        let error = checked(DEFAULT_CONFIG, "font_size", &string("big")).unwrap_err();
        assert!(error.to_string().starts_with("config.toml, line "));
    }

    #[test]
    fn every_setting_of_the_panel() {
        // Each one set in the file Kel writes reads back as set.
        let cases = [
            ("font", string("Cascadia Code")),
            ("font_size", float(20.0)),
            ("line_height", float(1.2)),
            ("ui_scale", float(1.25)),
            ("cursor", string("underline")),
            ("cursor_blink", Value::Boolean(false)),
            ("copy_on_select", Value::Boolean(true)),
            ("bell", string("flash")),
            ("start_screen", Value::Boolean(false)),
            ("backdrop", string("sea")),
            ("layout.corner_radius", Value::Integer(0)),
            ("layout.tab_bar", string("title")),
            ("glass.effect", string("mica")),
            ("glass.opacity", float(0.65)),
            ("glass.grain", float(0.0)),
        ];
        for (setting, value) in cases {
            let new = checked(DEFAULT_CONFIG, setting, &value).unwrap();
            assert_eq!(changed(DEFAULT_CONFIG, &new).len(), 1, "{setting}");
            let file = Value::Table(toml::from_str(&new).unwrap());
            let read = setting.split('.').fold(&file, |v, k| &v[k]);
            // 20.0 is written `20`: a number all the same.
            let number = |v: &Value| v.as_float().or(v.as_integer().map(|i| i as f64));
            match number(&value) {
                Some(x) => assert_eq!(number(read), Some(x), "{setting}"),
                None => assert_eq!(read, &value, "{setting}"),
            }
        }
    }
}
