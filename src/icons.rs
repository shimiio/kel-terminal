//! Tab icons: pixel art like Kel's logo, drawn here as grids of 8 x 8, a pixel
//! of them 2 x 2 on screen (`PIXEL`): big enough to see it's pixels. A tab's
//! icon is in the tab's color: the project's on the tab on screen (gray in
//! home), the muted gray on the others.
//!
//! ```text
//! k▌        >_        N         git       docker    ▶
//! #...###.  #.......  .#...#..  ##...##.  ...#....  .##.....
//! ```

use std::collections::HashMap;

/// A pixel of an icon's grid, in screen pixels at 100%: 2 x 2.
const PIXEL: usize = 2;

/// The icons: a name and its grid, 8 rows of 8, `#` lit.
const GRIDS: [(&str, [&str; 8]); 6] = [
    (
        "kel",
        [
            "........", "#...###.", "#...###.", "#.#.###.", "##..###.", "#.#.###.", "#.#.###.",
            "........",
        ],
    ),
    (
        "terminal",
        [
            "........", "#.......", ".#......", "..#.....", ".#......", "#.......", "...####.",
            "........",
        ],
    ),
    (
        "nvim",
        [
            "........", ".#...#..", ".##..#..", ".#.#.#..", ".#..##..", ".#...#..", ".#...#..",
            "........",
        ],
    ),
    (
        "git",
        [
            "##...##.", "##...##.", ".#...#..", ".#..#...", ".#.#....", ".##.....", "##......",
            "##......",
        ],
    ),
    (
        "docker",
        [
            "........", "...#....", ".#.#.#..", ".......#", "#######.", ".#####..", "..###...",
            "........",
        ],
    ),
    (
        "start",
        [
            "........", ".##.....", ".####...", ".######.", ".####...", ".##.....", "........",
            "........",
        ],
    ),
];

/// The icon of a tab whose title and name pick none.
const DEFAULT: &str = "terminal";

/// The tab of home's start screen: Kel's own mark, `k▌`.
pub const KEL: &str = "kel";

/// Words in a tab's title or name that pick an icon, checked in this order.
const WORDS: [(&str, &str); 7] = [
    ("nvim", "nvim"),
    ("vim", "nvim"),
    ("lazygit", "git"),
    ("git", "git"),
    ("docker", "docker"),
    ("start", "start"),
    ("run", "start"),
];

/// A tab's icon. First what runs in it, as its title says: a tab you called
/// "docker" shows nvim's icon while nvim is open in it. When the title says
/// nothing (a plain shell), the tab's `name`: a "docker" tab has the whale.
pub fn pick(title: &str, name: Option<&str>) -> &'static str {
    find(title)
        .or_else(|| name.and_then(find))
        .unwrap_or(DEFAULT)
}

/// The icon a text's words pick, if any. Words are compared whole, in any case:
/// "Cargo.toml - Nvim" and "nvim ." have nvim, "dsnvim" doesn't.
fn find(text: &str) -> Option<&'static str> {
    // Words: the pieces between anything that isn't a letter or a digit, e.g.
    // "Cargo.toml - Nvim" → "Cargo", "toml", "Nvim".
    let has_word = |word: &str| {
        text.split(|c: char| !c.is_alphanumeric())
            .any(|w| w.eq_ignore_ascii_case(word))
    };
    WORDS
        .iter()
        .find(|(word, _)| has_word(word))
        .map(|(_, icon)| *icon)
}

/// An icon: which of its pixels are lit, on screen at 100% (16 x 16).
pub struct Icon {
    pub w: usize,
    pub h: usize,
    /// Row after row.
    lit: Vec<bool>,
}

impl Icon {
    /// An icon from its grid: each `#` a lit square of `PIXEL` x `PIXEL`.
    fn from_grid(rows: &[&str]) -> Icon {
        let w = rows.first().map_or(0, |row| row.len()) * PIXEL;
        let h = rows.len() * PIXEL;
        let mut lit = Vec::with_capacity(w * h);
        for y in 0..h {
            let row = rows[y / PIXEL].as_bytes();
            lit.extend((0..w).map(|x| row.get(x / PIXEL) == Some(&b'#')));
        }
        Icon { w, h, lit }
    }

    /// Whether the pixel at (x, y) is lit.
    pub fn lit(&self, x: usize, y: usize) -> bool {
        self.lit[y * self.w + x]
    }
}

pub struct Icons(HashMap<&'static str, Icon>);

impl Icons {
    /// All the icons, from their grids.
    pub fn load() -> Icons {
        let icons = GRIDS
            .iter()
            .map(|(name, rows)| (*name, Icon::from_grid(rows)));
        Icons(icons.collect())
    }

    /// The icon named `name`, or the default one.
    pub fn get(&self, name: &str) -> &Icon {
        self.0.get(name).unwrap_or(&self.0[DEFAULT])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_grid_is_8_by_8() {
        // A short row would shift every row after it, and the last pixel asked
        // for wouldn't be there.
        for (name, rows) in GRIDS {
            assert!(rows.iter().all(|row| row.len() == 8), "{name}");
            assert!(
                rows.iter().all(|row| row.chars().all(|c| "#.".contains(c))),
                "{name}"
            );
        }
        let icons = Icons::load();
        assert_eq!((icons.get(KEL).w, icons.get(KEL).h), (16, 16));
    }

    #[test]
    fn a_grid_s_pixel_is_2_by_2() {
        let icon = Icons::load();
        let kel = icon.get(KEL);
        // Its row 1 is "#...###.": (0, 1) lit, so screen pixels 0..2, 2..4.
        assert!(kel.lit(0, 2) && kel.lit(1, 3));
        assert!(!kel.lit(2, 2)); // (1, 1): a dot
        assert!(kel.lit(8, 2) && kel.lit(13, 3)); // the cursor, 4..7
        assert!(!kel.lit(0, 0)); // row 0: empty
    }

    #[test]
    fn icon_from_the_title() {
        let for_title = |title| pick(title, None);
        assert_eq!(for_title("Cargo.toml - Nvim"), "nvim");
        assert_eq!(for_title("nvim ."), "nvim");
        assert_eq!(for_title("dsnvim"), "terminal");
        assert_eq!(for_title("lazygit"), "git");
        assert_eq!(for_title("powershell"), "terminal");
        // nvim comes before git in the list, even in a folder called "git".
        assert_eq!(for_title(r"main.rs (C:\git\kel) - Nvim"), "nvim");
    }

    #[test]
    fn icon_from_the_name_when_the_title_says_nothing() {
        assert_eq!(pick("powershell", Some("docker")), "docker");
        assert_eq!(pick("powershell", Some("Run")), "start");
        assert_eq!(pick("powershell", Some("dev server")), "terminal");
        // What runs comes first: nvim, in a tab called "docker".
        assert_eq!(pick("main.rs - Nvim", Some("docker")), "nvim");
    }
}
