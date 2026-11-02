//! Kel's logo: "kel" and a cursor after it, in pixels, like a terminal's prompt
//! waiting for you.
//!
//! ```text
//! #.......... ##. ####
//! #.......... .#. ####
//! #..#. .###. .#. ####
//! #.#.. #...# .#. ####
//! ##... ##### .#. ####
//! #.#.. #.... .#. ####
//! #..#. .###. .## ####
//! ```
//!
//! The letters are the text's color; the cursor is the project's (home: the
//! text's too, Kel has no color of its own). Big on the start screen, where the
//! cursor blinks; small in the sidebar, still. Only the pixels live here: the
//! renderer draws them at the size it wants.

/// The letters, a row of pixels a string, `#` lit.
const K: [&str; 7] = [
    "#....", "#....", "#..#.", "#.#..", "##...", "#.#..", "#..#.",
];
const E: [&str; 7] = [
    ".....", ".....", ".###.", "#...#", "#####", "#....", ".###.",
];
const L: [&str; 7] = ["##.", ".#.", ".#.", ".#.", ".#.", ".#.", ".##"];

/// The cursor's width; a pixel's gap between the letters, and before it.
const CURSOR_WIDTH: usize = 4;
const GAP: usize = 1;

/// The logo's height, and its width with the cursor, in its pixels.
pub const HEIGHT: usize = 7;
pub const WIDTH: usize = 5 + GAP + 5 + GAP + 3 + GAP + CURSOR_WIDTH;

/// A lit pixel of the logo: where (from its top left, in its pixels) and
/// whether it's the cursor's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pixel {
    pub x: usize,
    pub y: usize,
    pub cursor: bool,
}

/// The logo's lit pixels, the letters' first, then the cursor's.
pub fn pixels() -> Vec<Pixel> {
    let mut lit = Vec::new();
    let mut left = 0;
    for letter in [K, E, L] {
        for (y, row) in letter.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                if c == '#' {
                    lit.push(Pixel {
                        x: left + x,
                        y,
                        cursor: false,
                    });
                }
            }
        }
        left += letter[0].len() + GAP;
    }
    for y in 0..HEIGHT {
        for x in 0..CURSOR_WIDTH {
            lit.push(Pixel {
                x: left + x,
                y,
                cursor: true,
            });
        }
    }
    lit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_s_pixels() {
        let lit = pixels();
        // All inside, the cursor last: a block at the right.
        assert!(lit.iter().all(|p| p.x < WIDTH && p.y < HEIGHT));
        let cursor: Vec<_> = lit.iter().filter(|p| p.cursor).collect();
        assert_eq!(cursor.len(), CURSOR_WIDTH * HEIGHT);
        assert!(cursor.iter().all(|p| p.x >= WIDTH - CURSOR_WIDTH));
        // k's stem down the left; l's foot at the bottom.
        assert!((0..HEIGHT).all(|y| lit.contains(&Pixel {
            x: 0,
            y,
            cursor: false
        })));
        assert!(lit.contains(&Pixel {
            x: 14,
            y: 6,
            cursor: false
        }));
    }
}
