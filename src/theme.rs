//! Colors: what the program asks for (`Color`) and how it looks (`Theme`).
//!
//! A cell keeps the color the program asked for, like "palette color 1" or "the
//! default text color", not the final pixels. The renderer looks the color up in
//! the theme when it draws, so a new theme recolors text already on the screen.

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serializer};

/// A color as the program asked for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    /// The theme's text color (in a cell's `fg`) or background color (in its `bg`).
    Default,
    /// Color number n of the 256-color table, e.g. `ESC[31m` (red) = 1, `ESC[38;5;208m` = 208.
    Indexed(u8),
    /// An exact color `0xRRGGBB`, e.g. `ESC[38;2;255;128;0m` = 0xff8000 (orange).
    Rgb(u32),
}

/// How the colors look. Each one is `0xRRGGBB`.
///
/// It's the `[colors]` part of the config file, where colors are written
/// as "#rrggbb" (`deserialize_with` reads them). A missing color keeps its default.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Theme {
    #[serde(deserialize_with = "hex_color")]
    pub foreground: u32,
    #[serde(deserialize_with = "hex_color")]
    pub background: u32,
    #[serde(deserialize_with = "hex_color")]
    pub cursor: u32,
    #[serde(deserialize_with = "hex_color")]
    pub selection: u32,
    /// The window around the terminal panel.
    #[serde(deserialize_with = "hex_color")]
    pub frame: u32,
    /// The panel's border line.
    #[serde(deserialize_with = "hex_color")]
    pub border: u32,
    /// The color of home and of projects without their own: the logo's cursor,
    /// the tab icons. None (the default): no color, the text's gray; a project's
    /// own color is then the only color the window has. See `accent_for`.
    #[serde(deserialize_with = "optional_hex")]
    pub accent: Option<u32>,
    /// Kel's own colors, soft, all as bright: a project's color is picked among
    /// them (or is any #hex). Not the palette: programs color their text with
    /// that, and its colors are loud.
    #[serde(deserialize_with = "hex_list")]
    pub project_colors: Vec<u32>,
    /// A command that ended well: the dot on its tab.
    #[serde(deserialize_with = "hex_color")]
    pub done: u32,
    /// One that failed; and Kel's own errors (a bad config, nothing found).
    #[serde(deserialize_with = "hex_color")]
    pub failed: u32,
    /// The 16 basic colors: 0-7 normal (codes 30-37), 8-15 bright (codes 90-97).
    #[serde(deserialize_with = "hex_palette")]
    pub palette: [u32; 16],
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            foreground: 0xd4d4d4, // light gray
            background: 0x1e1e1e, // dark gray
            cursor: 0x808080,
            selection: 0x474747, // gray: never one of the search's or a project's colors
            frame: 0x141414,     // a bit darker than the background
            border: 0x333333,    // a bit lighter
            accent: None,        // none: home is gray
            project_colors: vec![
                0xe06c75, // red
                0xe5a46b, // orange
                0xe5c07b, // yellow
                0x98c379, // green
                0x6cc4b8, // teal
                0x7aa2e6, // blue
                0xb39ae6, // purple
                0xe690b8, // pink
            ],
            done: 0x98c379,   // the green
            failed: 0xe06c75, // the red
            palette: [
                // Soft, of the same family as `project_colors`; the bright ones lighter.
                0x3a3a3a, 0xe06c75, 0x98c379, 0xe5c07b, 0x7aa2e6, 0xb39ae6, 0x6cc4b8, 0xc8c8c8,
                0x6e6e6e, 0xf08a92, 0xb4d99a, 0xf2d49b, 0x9dbcf2, 0xcbb6f2, 0x8fd9cf, 0xececec,
            ],
        }
    }
}

impl Theme {
    /// The window's color where a project with `color` is on screen (`None`:
    /// home, or a project without one): its own color, else the theme's
    /// `accent`, else the text's gray. Kel has no color of its own: it takes
    /// the color of what you work on.
    pub fn accent_for(&self, color: Option<u32>) -> u32 {
        color.or(self.accent).unwrap_or(self.foreground)
    }

    /// The pixels for `color`. `default` is what `Color::Default` means here:
    /// `self.foreground` for text, `self.background` for a background.
    pub fn rgb(&self, color: Color, default: u32) -> u32 {
        match color {
            Color::Default => default,
            Color::Indexed(n) => self.indexed(n),
            Color::Rgb(rgb) => rgb,
        }
    }

    /// Color number n of the 256-color table. 0-15 come from the theme, the rest
    /// are the same in every terminal.
    fn indexed(&self, n: u8) -> u32 {
        match n {
            0..=15 => self.palette[n as usize],
            16..=231 => {
                // The 6x6x6 cube: n - 16 = 36*r + 6*g + b, each a level 0-5.
                let i = (n - 16) as u32;
                let level = |v: u32| if v == 0 { 0 } else { 55 + 40 * v };
                (level(i / 36) << 16) | (level(i / 6 % 6) << 8) | level(i % 6)
            }
            232..=255 => {
                // 24 grays, from dark to light.
                let v = 8 + 10 * (n - 232) as u32;
                (v << 16) | (v << 8) | v
            }
        }
    }
}

/// "#ff8000" → 0xff8000. `None` if it isn't `#` and 6 hex digits.
pub fn parse_hex(text: &str) -> Option<u32> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

/// Reads one color from the config: first the text, then `parse_hex` on it.
fn hex_color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let text = String::deserialize(deserializer)?;
    parse_hex(&text)
        .ok_or_else(|| D::Error::custom(format!("\"{text}\" is not a color like \"#ff8000\"")))
}

/// Reads a color that may be missing (with `#[serde(default)]`), e.g. a project's
/// `color`: there, it's `Some`.
pub fn optional_hex<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u32>, D::Error> {
    hex_color(deserializer).map(Some)
}

/// Writes a color back as the file has it: 0xe0af68 → "#e0af68".
pub fn write_hex<S: Serializer>(color: &Option<u32>, serializer: S) -> Result<S::Ok, S::Error> {
    match color {
        Some(color) => serializer.serialize_str(&format!("#{color:06x}")),
        None => serializer.serialize_none(),
    }
}

/// Reads a list of colors, as many as there are.
fn hex_list<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u32>, D::Error> {
    let texts = Vec::<String>::deserialize(deserializer)?;
    texts
        .iter()
        .map(|text| {
            parse_hex(text).ok_or_else(|| {
                D::Error::custom(format!("\"{text}\" is not a color like \"#ff8000\""))
            })
        })
        .collect()
}

/// Reads the palette from the config: exactly 16 colors.
fn hex_palette<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u32; 16], D::Error> {
    let texts = <[String; 16]>::deserialize(deserializer)?;
    let mut palette = [0; 16];
    for (color, text) in palette.iter_mut().zip(&texts) {
        *color = parse_hex(text).ok_or_else(|| {
            D::Error::custom(format!("\"{text}\" is not a color like \"#ff8000\""))
        })?;
    }
    Ok(palette)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors() {
        assert_eq!(parse_hex("#ff8000"), Some(0xff8000));
        assert_eq!(parse_hex("#FF8000"), Some(0xff8000));
        assert_eq!(parse_hex("ff8000"), None); // no #
        assert_eq!(parse_hex("#f80"), None); // too short
        assert_eq!(parse_hex("#+f8000"), None); // not hex digits
    }

    #[test]
    fn the_window_s_color() {
        let mut theme = Theme::default();
        // No accent: home is the text's gray, a project its own color.
        assert_eq!(theme.accent_for(None), theme.foreground);
        assert_eq!(theme.accent_for(Some(0xe0af68)), 0xe0af68);
        // An accent set: home's, and of projects without their own.
        theme.accent = Some(0x569cd6);
        assert_eq!(theme.accent_for(None), 0x569cd6);
        assert_eq!(theme.accent_for(Some(0xe0af68)), 0xe0af68);
    }

    #[test]
    fn default_depends_on_text_or_background() {
        let theme = Theme::default();
        assert_eq!(theme.rgb(Color::Default, theme.foreground), 0xd4d4d4);
        assert_eq!(theme.rgb(Color::Default, theme.background), 0x1e1e1e);
    }

    #[test]
    fn indexed_colors() {
        let theme = Theme::default();
        assert_eq!(theme.rgb(Color::Indexed(1), 0), theme.palette[1]); // from the theme
        assert_eq!(theme.rgb(Color::Indexed(196), 0), 0xff0000); // pure red, from the cube
        assert_eq!(theme.rgb(Color::Indexed(244), 0), 0x808080); // a middle gray
    }

    #[test]
    fn a_new_palette_recolors_the_same_color() {
        let mut theme = Theme::default();
        theme.palette[1] = 0xff5555;
        assert_eq!(theme.rgb(Color::Indexed(1), 0), 0xff5555);
    }
}
