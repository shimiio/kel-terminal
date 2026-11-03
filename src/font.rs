//! Fonts: finding one among the installed fonts, and turning its letters into
//! coverage maps (how much of each pixel a letter covers, 0 to 255).
//!
//! A font is kept as its file's bytes (ab_glyph), and a letter is read from the
//! file only when it's drawn the first time; the renderer's glyph cache keeps the
//! result. Kel used fontdue before, which reads every letter of the file up front:
//! JetBrainsMono Nerd Font has ~12 600 (most of them icons), 4 styles, and Kel
//! took 213 MB (measured, release build; 453 MB once an emoji and a Chinese
//! letter loaded their fallback fonts). The files themselves are 4 × 2.6 MB.
//!
//! Sizes are as fontdue had them, so `font_size` means the same: `size` pixels
//! is the font's em, the square its letters are designed in.
//!
//! Built into the exe (`assets/fonts`, ~3.5 MB): JetBrains Mono in 4 styles, Kel's
//! own font (its UI always, the terminal's unless the config names another), and
//! the Nerd Font icons alone (Symbols Nerd Font Mono), which any font gets.

use ab_glyph::{Font as _, FontVec, GlyphId, PxScale};
use anyhow::Context;
use fontdb::{Family, Query, Style, Weight};

/// One font family in 4 styles. Index = bold + 2 * italic:
/// 0 = regular, 1 = bold, 2 = italic, 3 = bold italic.
pub type Fonts = [Font; 4];

/// One font file (or one font of a `.ttc` file, which holds several).
pub struct Font(FontVec);

/// The built-in font's name: `font` in the config by this name (any case) is
/// it, installed or not.
pub const BUILT_IN: &str = "JetBrains Mono";

/// Its files, in the order of `Fonts`: regular, bold, italic, bold italic.
const BUILT_IN_FILES: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
];

/// The Nerd Font icons ("Mono": each a square of 1 em, ~1.7 of JetBrains Mono's
/// cells; the renderer lets them reach into the next cell, see `glyph`).
const SYMBOLS_FILE: &[u8] = include_bytes!("../assets/fonts/SymbolsNerdFontMono-Regular.ttf");

/// The built-in font, its 4 styles.
pub fn built_in() -> Fonts {
    BUILT_IN_FILES.map(|data| Font::from_bytes(data.to_vec(), 0).expect("the built-in font"))
}

/// The built-in icons.
pub fn symbols() -> Font {
    Font::from_bytes(SYMBOLS_FILE.to_vec(), 0).expect("the built-in icons")
}

/// Where a rasterized letter goes, relative to where it's drawn: the pen on the
/// baseline, the line letters sit on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    /// From the pen to the coverage map's left edge, in pixels.
    pub xmin: i32,
    /// From the baseline up to the map's bottom edge: negative for 'g', 'p'...
    pub ymin: i32,
    /// The coverage map's size.
    pub width: usize,
    pub height: usize,
    /// How far the pen moves after the letter.
    pub advance_width: f32,
}

/// A line of text at one size, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineMetrics {
    /// From the baseline up to the top of the tallest letters.
    pub ascent: f32,
    /// From the baseline down to the bottom of 'g' (negative).
    pub descent: f32,
    /// Extra space the font asks for between lines.
    pub line_gap: f32,
    /// One line to the next: ascent - descent + line_gap.
    pub new_line_size: f32,
}

impl Font {
    /// `index`: which font of a `.ttc` file (0 for a plain `.ttf`).
    pub fn from_bytes(data: Vec<u8>, index: u32) -> anyhow::Result<Font> {
        let font = FontVec::try_from_vec_and_index(data, index)?;
        Ok(Font(font))
    }

    /// Whether the font has a letter for `c` (from its character map alone).
    pub fn has_glyph(&self, c: char) -> bool {
        self.0.glyph_id(c) != GlyphId(0)
    }

    /// How far the pen moves after `c`, at `size`.
    pub fn advance(&self, c: char, size: f32) -> f32 {
        self.0.h_advance_unscaled(self.0.glyph_id(c)) * self.per_unit(size)
    }

    pub fn line_metrics(&self, size: f32) -> LineMetrics {
        let k = self.per_unit(size);
        let ascent = self.0.ascent_unscaled() * k;
        let descent = self.0.descent_unscaled() * k;
        let line_gap = self.0.line_gap_unscaled() * k;
        LineMetrics {
            ascent,
            descent,
            line_gap,
            new_line_size: ascent - descent + line_gap,
        }
    }

    /// `c` at `size` as a coverage map, row after row, and where it goes. A letter
    /// the font lacks: its "missing" box (letter 0). A space: an empty map.
    pub fn rasterize(&self, c: char, size: f32) -> (Metrics, Vec<u8>) {
        let id = self.0.glyph_id(c);
        let mut metrics = Metrics {
            advance_width: self.advance(c, size),
            ..Metrics::default()
        };
        let glyph = id.with_scale(self.scale(size));
        let Some(outline) = self.0.outline_glyph(glyph) else {
            return (metrics, Vec::new()); // nothing to draw: a space
        };
        // The map's corners in whole pixels, y going down from the baseline:
        // e.g. 'g' at 18: min.y = -10 (above the baseline), max.y = 4 (below it).
        let bounds = outline.px_bounds();
        metrics.xmin = bounds.min.x as i32;
        metrics.ymin = -(bounds.max.y as i32);
        metrics.width = bounds.width() as usize;
        metrics.height = bounds.height() as usize;
        let mut coverage = vec![0; metrics.width * metrics.height];
        outline.draw(|x, y, cover| {
            // Overlapping outlines can add up past 1.0: still a full pixel.
            coverage[y as usize * metrics.width + x as usize] = (cover.min(1.0) * 255.0) as u8;
        });
        (metrics, coverage)
    }

    /// The font's units (its letters are drawn in, e.g. 2048 per em) to pixels.
    fn per_unit(&self, size: f32) -> f32 {
        size / self.0.units_per_em().unwrap_or(1000.0)
    }

    /// ab_glyph's scale for `size`. Its scale is the height from ascent to
    /// descent, not the em: e.g. Consolas' line is 1.17 em, so 18 → 21.08.
    fn scale(&self, size: f32) -> PxScale {
        PxScale::from(self.0.height_unscaled() * self.per_unit(size))
    }
}

/// Finds the font family `family` (e.g. "Consolas") among the installed fonts and
/// loads its 4 styles. A family without, say, an italic file gets the closest style.
pub fn load_family(family: &str) -> anyhow::Result<Fonts> {
    if family.eq_ignore_ascii_case(BUILT_IN) {
        return Ok(built_in());
    }
    let mut db = fontdb::Database::new();
    // Reads the names of all installed fonts, in C:\Windows\Fonts and the user's
    // own fonts folder. About 20 ms (the first time after a reboot, ~1 s: from disk).
    db.load_system_fonts();

    let load = |weight: Weight, style: Style| -> anyhow::Result<Font> {
        let query = Query {
            families: &[Family::Name(family)],
            weight,
            style,
            ..Default::default()
        };
        let id = db
            .query(&query)
            .with_context(|| format!("font \"{family}\" is not installed"))?;
        // A .ttc file holds several fonts; `index` says which one is ours.
        db.with_face_data(id, |data, index| Font::from_bytes(data.to_vec(), index))
            .with_context(|| format!("can't read font \"{family}\""))?
    };

    // Same order as the index: bold + 2 * italic.
    Ok([
        load(Weight::NORMAL, Style::Normal)?,
        load(Weight::BOLD, Style::Normal)?,
        load(Weight::NORMAL, Style::Italic)?,
        load(Weight::BOLD, Style::Italic)?,
    ])
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Consolas comes with every Windows.
    fn consolas() -> Font {
        let [regular, ..] = load_family("Consolas").unwrap();
        regular
    }

    /// The numbers fontdue gave (in `renderer::measure_cell`'s comment), so a
    /// config's `font_size` makes the same cells as before.
    #[test]
    fn sizes_as_before() {
        let font = consolas();
        let line = font.line_metrics(18.0);
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(
            close(font.advance('M', 18.0), 9.90),
            "{}",
            font.advance('M', 18.0)
        );
        assert!(close(line.new_line_size, 21.08), "{line:?}");
        assert!(close(line.ascent, 13.37), "{line:?}");
        assert!(close(line.line_gap / 2.0, 1.54), "{line:?}");
    }

    #[test]
    fn letters_sit_on_the_baseline() {
        let font = consolas();
        let (x, x_map) = font.rasterize('x', 18.0);
        assert_eq!(x.ymin, 0, "{x:?}"); // from the baseline up
        assert_eq!(x_map.len(), x.width * x.height);
        assert!(x_map.contains(&255));
        let (g, _) = font.rasterize('g', 18.0);
        assert!(g.ymin < 0, "{g:?}"); // below it
        assert!(g.height > x.height);
    }

    #[test]
    fn the_built_in_font() {
        // By its name, in any case: the files in the exe, not Windows' fonts.
        let [regular, bold, ..] = load_family("jetbrains mono").unwrap();
        assert!(regular.has_glyph('A') && regular.has_glyph('ж'));
        assert!(bold.has_glyph('A'));
        // Icons aren't in it (only powerline's arrows, U+E0A0...); the built-in
        // icons have them, each a square of 1 em: wider than a letter.
        let folder = '\u{F07B}'; // Nerd Font's (Font Awesome's) folder
        assert!(!regular.has_glyph(folder));
        let icons = symbols();
        assert!(icons.has_glyph(folder) && icons.has_glyph('\u{F418}')); // and a git branch
        assert!((icons.advance(folder, 18.0) - 18.0).abs() < 0.5);
        assert!(regular.advance('M', 18.0) < 12.0);
    }

    #[test]
    fn a_space_and_a_missing_letter() {
        let font = consolas();
        let (space, map) = font.rasterize(' ', 18.0);
        assert!(map.is_empty());
        assert!(space.advance_width > 9.0);
        assert!(font.has_glyph('A'));
        assert!(!font.has_glyph('中'));
        // Missing: the "missing" box, not nothing.
        let (_, missing) = font.rasterize('中', 18.0);
        assert!(!missing.is_empty());
    }
}
