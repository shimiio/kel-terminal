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
//! cursor blinks; small in the sidebar, blinking while it's open (the start
//! screen's stands then: a cursor blinks where the keys go). And `k▌` alone, the window's
//! icon in the taskbar (see `icon`), its cursor the project's color too. The
//! renderer draws the pixels at the size it wants.

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

/// The logo's height, and its width with the cursor, in its pixels: from the
/// letters, so a letter drawn wider or narrower moves the cursor with it.
pub const HEIGHT: usize = 7;
pub const WIDTH: usize = K[0].len() + GAP + E[0].len() + GAP + L[0].len() + GAP + CURSOR_WIDTH;

/// The icon's mark, `k▌`: its width.
const MARK_WIDTH: usize = K[0].len() + GAP + CURSOR_WIDTH;

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
    lay_out(&[K, E, L])
}

/// `letters` side by side, a gap between, then the cursor: their lit pixels.
fn lay_out(letters: &[[&str; HEIGHT]]) -> Vec<Pixel> {
    let mut lit = Vec::new();
    let mut left = 0;
    for letter in letters {
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

/// The window's icon, `size` pixels square, as RGBA bytes: `k▌` in the middle
/// of a square of `back` with round corners, the k in `letters`, the cursor in
/// `cursor`. A pixel of the mark is `size / 16` pixels (rounded): 10 x 7 of 16.
pub fn icon(size: usize, letters: u32, cursor: u32, back: u32) -> Vec<u8> {
    let px = (size + 8) / 16;
    let px = px.max(1);
    let left = size.saturating_sub(MARK_WIDTH * px) / 2;
    let top = size.saturating_sub(HEIGHT * px) / 2;
    let mut colors = vec![back; size * size];
    for p in lay_out(&[K]) {
        let color = if p.cursor { cursor } else { letters };
        for y in top + p.y * px..top + (p.y + 1) * px {
            for x in left + p.x * px..left + (p.x + 1) * px {
                colors[y * size + x] = color;
            }
        }
    }
    let radius = size as f32 / 6.0;
    let mut rgba = Vec::with_capacity(size * size * 4);
    for (i, color) in colors.into_iter().enumerate() {
        let alpha = (corner_cover(i % size, i / size, size, radius) * 255.0).round() as u8;
        rgba.extend([(color >> 16) as u8, (color >> 8) as u8, color as u8, alpha]);
    }
    rgba
}

/// How much of pixel (x, y) is inside a square of `size` with corners round by
/// `radius`: 1 inside, 0 out, between on a corner's edge (smooth, not stairs).
fn corner_cover(x: usize, y: usize, size: usize, radius: f32) -> f32 {
    // The pixel's middle, and the nearest corner's circle's middle.
    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
    let far = size as f32 - radius;
    let cx = px.clamp(radius, far);
    let cy = py.clamp(radius, far);
    let distance = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
    (radius - distance + 0.5).clamp(0.0, 1.0)
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

    #[test]
    fn the_icon() {
        let (fg, cursor, back) = (0xd4d4d4, 0xe0af68, 0x1e1e1e);
        let icon = icon(16, fg, cursor, back);
        assert_eq!(icon.len(), 16 * 16 * 4);
        let at = |x: usize, y: usize| &icon[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4];
        // k▌ is 10 x 7, in the middle: from (3, 4). Its stem, its cursor.
        assert_eq!(at(3, 4), [0xd4, 0xd4, 0xd4, 255]);
        assert_eq!(at(9, 10), [0xe0, 0xaf, 0x68, 255]);
        assert_eq!(at(2, 4), [0x1e, 0x1e, 0x1e, 255]); // the square
        // A round corner: mostly see-through (its edge smooth); a side's middle,
        // all there.
        assert!(at(0, 0)[3] < 64);
        assert_eq!(at(0, 8)[3], 255);
        // At 32, a pixel of it is 2: from ((32 - 20) / 2, (32 - 14) / 2) = (6, 9).
        let big = super::icon(32, fg, cursor, back);
        assert_eq!(&big[(9 * 32 + 6) * 4..][..4], [0xd4, 0xd4, 0xd4, 255]);
    }
}
