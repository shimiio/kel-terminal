//! Fallback fonts: for the characters the terminal's font doesn't have. First
//! Kel's built-in icons (Nerd Font's, which nvim plugins use: any font gets
//! them), then fonts that come with Windows (emoji, Chinese, Japanese, Korean).
//!
//! They're big (Segoe UI Emoji 12 MB, Microsoft YaHei 19 MB), so nothing is
//! loaded at start, and each font only when a character first needs it. Which
//! font has the character: its character map, read with ttf-parser without
//! loading the rest (fast).

use crate::font::{self, Font};
use fontdb::{Database, Family, ID, Query};

/// Tried in this order: emoji, symbols, Chinese (also Japanese kana), Korean.
const FAMILIES: [&str; 4] = [
    "Segoe UI Emoji",
    "Segoe UI Symbol",
    "Microsoft YaHei",
    "Malgun Gothic",
];

/// Each `None` until the first character needs it.
#[derive(Default)]
pub struct Fallbacks {
    /// The built-in icons.
    symbols: Option<Font>,
    installed: Option<Installed>,
}

/// The installed fonts' names, and ours among them, in `FAMILIES` order.
struct Installed {
    db: Database,
    fonts: Vec<Fallback>,
}

struct Fallback {
    id: ID,
    /// Segoe UI Emoji: see `font_for`.
    emoji: bool,
    /// Loaded the first time a character needs it.
    font: Option<Font>,
}

impl Fallbacks {
    /// The first fallback font that has `c`, loaded now if it wasn't yet.
    /// Whether `c` is one of the built-in icons.
    pub fn is_icon(&mut self, c: char) -> bool {
        self.symbols.get_or_insert_with(font::symbols).has_glyph(c)
    }

    pub fn font_for(&mut self, c: char) -> Option<&Font> {
        if self.is_icon(c) {
            return self.symbols.as_ref();
        }
        let Installed { db, fonts } = self.installed.get_or_insert_with(Installed::find);
        // A symbol older than emoji (`✳ ❤ ☀`, below U+1F000) is text first: the
        // emoji font has it too, but drawn for color, and without the color it's
        // e.g. a star in a box. Another font's plain one, if there is one.
        let text = (c as u32) < 0x1F000;
        let i = fonts
            .iter()
            .position(|f| !(text && f.emoji) && has(db, f.id, c))
            .or_else(|| fonts.iter().position(|f| has(db, f.id, c)))?;
        let fallback = &mut fonts[i];
        if fallback.font.is_none() {
            fallback.font = db.with_face_data(fallback.id, load).flatten();
        }
        fallback.font.as_ref()
    }
}

impl Installed {
    /// Looks up which of `FAMILIES` are installed (~20 ms: reads the fonts' names).
    fn find() -> Installed {
        let mut db = Database::new();
        db.load_system_fonts();
        let fonts = FAMILIES
            .iter()
            .filter_map(|family| {
                let query = Query {
                    families: &[Family::Name(family)],
                    ..Default::default()
                };
                db.query(&query).map(|id| Fallback {
                    id,
                    emoji: *family == "Segoe UI Emoji",
                    font: None,
                })
            })
            .collect();
        Installed { db, fonts }
    }
}

/// Whether the font `id` has a glyph for `c`, from its character map alone.
fn has(db: &Database, id: ID, c: char) -> bool {
    db.with_face_data(id, |data, index| {
        ttf_parser::Face::parse(data, index).is_ok_and(|face| face.glyph_index(c).is_some())
    })
    .unwrap_or(false)
}

fn load(data: &[u8], index: u32) -> Option<Font> {
    Font::from_bytes(data.to_vec(), index).ok()
}
