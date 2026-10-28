//! Colors: what the program asks for (`Color`) and how it looks (`Theme`).
//!
//! A cell keeps the color the program asked for, like "palette color 1" or "the
//! default text color", not the final pixels. The renderer looks the color up in
//! the theme when it draws, so a new theme recolors text already on the screen.

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
pub struct Theme {
    pub foreground: u32,
    pub background: u32,
    pub cursor: u32,
    pub selection: u32,
    /// The 16 basic colors: 0-7 normal (codes 30-37), 8-15 bright (codes 90-97).
    pub palette: [u32; 16],
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            foreground: 0xd4d4d4, // light gray
            background: 0x1e1e1e, // dark gray
            cursor: 0x808080,
            selection: 0x264f78, // blue
            palette: [
                0x0c0c0c, 0xc50f1f, 0x13a10e, 0xc19c00, 0x0037da, 0x881798, 0x3a96dd, 0xcccccc,
                0x767676, 0xe74856, 0x16c60c, 0xf9f1a5, 0x3b78ff, 0xb4009e, 0x61d6d6, 0xf2f2f2,
            ],
        }
    }
}

impl Theme {
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

#[cfg(test)]
mod tests {
    use super::*;

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
