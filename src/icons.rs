//! Tab icons: small pixel-art PNGs from `assets/icons`, built into Kel's .exe.
//!
//! An icon drawn only in grays is tinted: each pixel becomes the accent color times
//! its brightness, so white is the accent itself and 50% gray is half as bright.
//! One file then fits any color. An icon with real colors is drawn as it is.
//! On a muted tab every icon, colored or not, is tinted with the muted text color.

use anyhow::Context;
use std::collections::HashMap;

/// The icons: a name and the PNG file. `include_bytes!` puts the file's bytes into
/// the .exe when Kel is built, so Kel needs no files next to it.
const FILES: [(&str, &[u8]); 3] = [
    ("terminal", include_bytes!("../assets/icons/terminal.png")),
    ("nvim", include_bytes!("../assets/icons/nvim.png")),
    ("git", include_bytes!("../assets/icons/git.png")),
];

/// The icon of a tab whose title picks none.
const DEFAULT: &str = "terminal";

/// Words in a tab's title that pick an icon, checked in this order.
const WORDS: [(&str, &str); 4] = [
    ("nvim", "nvim"),
    ("vim", "nvim"),
    ("lazygit", "git"),
    ("git", "git"),
];

/// The icon for a tab's title. The title's words are compared whole, in any case:
/// "Cargo.toml - Nvim" and "nvim ." get the nvim icon, "dsnvim" doesn't.
pub fn for_title(title: &str) -> &'static str {
    // Words: the pieces between anything that isn't a letter or a digit, e.g.
    // "Cargo.toml - Nvim" → "Cargo", "toml", "Nvim".
    let has_word = |word: &str| {
        title
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w.eq_ignore_ascii_case(word))
    };
    WORDS
        .iter()
        .find(|(word, _)| has_word(word))
        .map_or(DEFAULT, |(_, icon)| icon)
}

pub struct Icon {
    pub w: usize,
    pub h: usize,
    /// [red, green, blue, alpha] per pixel, row after row. Alpha 0 is transparent.
    pixels: Vec<[u8; 4]>,
    /// Only grays: tinted with the accent color.
    pub tinted: bool,
}

impl Icon {
    /// The pixel at (x, y): its color `0xRRGGBB` and its alpha, 0-255.
    /// With a `tint`, the pixel's brightness times the tint; without, its own color.
    /// e.g. gray 128 with the tint 0x4080ff: 0x204080.
    pub fn pixel(&self, x: usize, y: usize, tint: Option<u32>) -> (u32, u8) {
        let [r, g, b, a] = self.pixels[y * self.w + x];
        let (r, g, b) = (r as u32, g as u32, b as u32);
        let color = match tint {
            Some(tint) => {
                // How bright the pixel looks: green counts most, blue least
                // (77 + 150 + 29 = 256). For a gray, it's the gray itself.
                let brightness = (77 * r + 150 * g + 29 * b) / 256;
                let times = |channel: u32| channel * brightness / 255;
                let (tr, tg, tb) = ((tint >> 16) & 0xff, (tint >> 8) & 0xff, tint & 0xff);
                (times(tr) << 16) | (times(tg) << 8) | times(tb)
            }
            None => (r << 16) | (g << 8) | b,
        };
        (color, a)
    }
}

pub struct Icons(HashMap<&'static str, Icon>);

impl Icons {
    /// Reads all the icons. They are part of Kel itself, so a broken one is a bug,
    /// not a user's mistake (a test reads them all).
    pub fn load() -> Icons {
        Icons(
            FILES
                .iter()
                .map(|(name, png)| (*name, decode(png).expect("a built-in icon")))
                .collect(),
        )
    }

    /// The icon named `name`, or the default one.
    pub fn get(&self, name: &str) -> &Icon {
        self.0.get(name).unwrap_or(&self.0[DEFAULT])
    }
}

/// Unpacks a PNG file into pixels.
fn decode(png: &[u8]) -> anyhow::Result<Icon> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    // Whatever the file holds (16-bit, a palette of colors...): 8-bit channels.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().context("icon too big")?];
    let info = reader.next_frame(&mut buf)?;
    let data = &buf[..info.buffer_size()];
    // Aseprite saves gray icons as gray + alpha, 2 bytes a pixel.
    let pixels: Vec<[u8; 4]> = match info.color_type {
        png::ColorType::Grayscale => data.iter().map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Rgba => data
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect(),
        png::ColorType::Indexed => anyhow::bail!("palette PNG left unexpanded"),
    };
    let tinted = pixels
        .iter()
        .all(|&[r, g, b, a]| a == 0 || (r == g && g == b));
    Ok(Icon {
        w: info.width as usize,
        h: info.height as usize,
        pixels,
        tinted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_icons_load() {
        let icons = Icons::load();
        for (name, _) in FILES {
            let icon = icons.get(name);
            assert_eq!((icon.w, icon.h), (16, 16), "{name}");
            assert!(icon.tinted, "{name}");
        }
    }

    #[test]
    fn icon_from_the_title() {
        assert_eq!(for_title("Cargo.toml - Nvim"), "nvim");
        assert_eq!(for_title("nvim ."), "nvim");
        assert_eq!(for_title("dsnvim"), "terminal");
        assert_eq!(for_title("lazygit"), "git");
        assert_eq!(for_title("powershell"), "terminal");
        // nvim comes before git in the list, even in a folder called "git".
        assert_eq!(for_title(r"main.rs (C:\git\kel) - Nvim"), "nvim");
    }

    #[test]
    fn tinted_pixel() {
        let icon = Icon {
            w: 1,
            h: 1,
            pixels: vec![[128, 128, 128, 255]],
            tinted: true,
        };
        assert_eq!(icon.pixel(0, 0, Some(0x4080ff)), (0x204080, 255));
        assert_eq!(icon.pixel(0, 0, None), (0x808080, 255));
    }
}
