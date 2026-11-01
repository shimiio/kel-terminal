//! Fallback fonts: for the characters the terminal's font doesn't have (emoji,
//! Chinese, Japanese, Korean), fonts that come with Windows.
//!
//! Loading one is slow: fontdue reads every glyph up front (measured: Segoe UI
//! Emoji, 12 MB, ~230 ms; Microsoft YaHei, 19 MB, ~460 ms). So nothing is loaded
//! at start, and each font only when a character first needs it. Which font has
//! the character: its character map, read with ttf-parser without loading the
//! rest (fast).

use fontdb::{Database, Family, ID, Query};

/// Tried in this order: emoji, symbols, Chinese (also Japanese kana), Korean.
const FAMILIES: [&str; 4] = [
    "Segoe UI Emoji",
    "Segoe UI Symbol",
    "Microsoft YaHei",
    "Malgun Gothic",
];

/// `None` until the first character needs a fallback.
#[derive(Default)]
pub struct Fallbacks(Option<Installed>);

/// The installed fonts' names, and ours among them, in `FAMILIES` order.
struct Installed {
    db: Database,
    fonts: Vec<Fallback>,
}

struct Fallback {
    id: ID,
    /// Loaded the first time a character needs it.
    font: Option<fontdue::Font>,
}

impl Fallbacks {
    /// The first fallback font that has `c`, loaded now if it wasn't yet.
    pub fn font_for(&mut self, c: char) -> Option<&fontdue::Font> {
        let Installed { db, fonts } = self.0.get_or_insert_with(Installed::find);
        let fallback = fonts.iter_mut().find(|fallback| has(db, fallback.id, c))?;
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
                db.query(&query).map(|id| Fallback { id, font: None })
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

fn load(data: &[u8], index: u32) -> Option<fontdue::Font> {
    let settings = fontdue::FontSettings {
        collection_index: index,
        ..Default::default()
    };
    fontdue::Font::from_bytes(data, settings).ok()
}
