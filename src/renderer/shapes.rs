//! Shapes and colors: rectangles, round corners, circles, boxes, icons, and
//! mixing one color over another (over glass too).

use super::PANE_BLACK;
use crate::glass::Glass;
use crate::icons::Icon;
use crate::layout::{Layout, Rect};
use crate::theme::Theme;

/// The theme glass makes, its background solid again (`solid_color`, or the
/// theme's): the frame around the panel stays glass.
pub(super) fn solid(glass: &Glass, theme: &Theme) -> Theme {
    Theme {
        background: glass.solid_color.unwrap_or(theme.background),
        ..glass.theme(theme)
    }
}

/// A rectangle's outline, `t` pixels thick.
/// The color of a box over the terminal: the palette, the help, a card, the search.
pub(super) fn pane(theme: &Theme, layout: &Layout) -> u32 {
    match layout.border {
        0 => blend(theme.frame, 0x000000, PANE_BLACK),
        _ => theme.frame,
    }
}

/// A box over the terminal at `r`: its line, if there is one, and `back` inside.
pub(super) fn draw_box(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    r: Rect,
    back: u32,
) {
    let (b, radius) = (layout.border, layout.radius);
    if b > 0 {
        fill_round(pixels, width, r, [radius; 4], theme.border);
    }
    let inner = radius.saturating_sub(b);
    fill_round(pixels, width, r.shrink(b), [inner; 4], back);
}

/// How much of a pixel a round corner covers, 0 to 255. The pixel is `a` columns
/// and `b` rows in from the corner of a `radius` square: its middle's distance
/// from the circle's center says it, and the last pixel's worth is the soft edge.
/// e.g. radius 8: the very corner, (0, 0), is 0; (7, 7), by the center, 255.
fn corner_cover(radius: usize, a: usize, b: usize) -> u32 {
    let n = radius as f32;
    let (dx, dy) = (n - a as f32 - 0.5, n - b as f32 - 0.5);
    let edge = n - (dx * dx + dy * dy).sqrt() + 0.5;
    (edge.clamp(0.0, 1.0) * 255.0).round() as u32
}

/// `r` filled, its corners round: `radii` from the top left, clockwise. A corner's
/// pixels mix with what's under them, so its edge is soft, over glass too.
pub(super) fn fill_round(pixels: &mut [u32], width: usize, r: Rect, radii: [usize; 4], color: u32) {
    let [tl, tr, br, bl] = radii.map(|n| n.min(r.w / 2).min(r.h / 2));
    let (top_h, bottom_h) = (tl.max(tr), br.max(bl));
    // Between the corners: plain rows.
    fill_rect(
        pixels,
        width,
        r.x,
        r.y + top_h,
        r.w,
        r.h.saturating_sub(top_h + bottom_h),
        color,
    );
    let height = pixels.len() / width;
    let rows = (0..top_h).chain(r.h.saturating_sub(bottom_h)..r.h);
    for y in rows.filter(|y| r.y + y < height) {
        let up = r.h - 1 - y; // rows to the bottom edge
        let row = &mut pixels[(r.y + y) * width..][..width];
        for x in 0..r.w.min(width.saturating_sub(r.x)) {
            let back = r.w - 1 - x; // columns to the right edge
            let cover = match () {
                _ if x < tl && y < tl => corner_cover(tl, x, y),
                _ if back < tr && y < tr => corner_cover(tr, back, y),
                _ if back < br && up < br => corner_cover(br, back, up),
                _ if x < bl && up < bl => corner_cover(bl, x, up),
                _ => 255,
            };
            let pixel = &mut row[r.x + x];
            *pixel = blend(*pixel, color, cover);
        }
    }
}

pub(super) fn outline(pixels: &mut [u32], width: usize, r: Rect, t: usize, color: u32) {
    fill_rect(pixels, width, r.x, r.y, r.w, t, color); // top
    fill_rect(pixels, width, r.x, r.y + r.h - t, r.w, t, color); // bottom
    fill_rect(pixels, width, r.x, r.y, t, r.h, color); // left
    fill_rect(pixels, width, r.x + r.w - t, r.y, t, r.h, color); // right
}

pub(super) fn fill_rect(
    pixels: &mut [u32],
    width: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: u32,
) {
    // Only the part inside the window: in a very small window the panel or the
    // grid can stick out, and a pixel past the edge would land on the next row.
    let height = pixels.len() / width;
    for py in y..(y + h).min(height) {
        for px in x..(x + w).min(width) {
            pixels[py * width + px] = color;
        }
    }
}

/// Characters that only steer others and show nothing: the joiner in emoji
/// sequences (ZWJ, U+200D), FE0F ("as an emoji") and the like. Some fonts still
/// have a mark for them (Segoe UI Emoji's ZWJ: a bar 4 x 20 pixels). ConPTY gives
/// each a cell (see `char_width`), which stays empty.
pub(super) fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}'
    )
}

/// Draws `icon` with its top-left corner at (x, y), its lit pixels in `color`.
/// Each of its pixels becomes a `k` x `k` square: at ×2, a 16x16 icon is 32x32.
pub(super) fn draw_icon(
    pixels: &mut [u32],
    width: usize,
    icon: &Icon,
    k: usize,
    color: u32,
    x: usize,
    y: usize,
) {
    for iy in 0..icon.h {
        for ix in 0..icon.w {
            if icon.lit(ix, iy) {
                fill_rect(pixels, width, x + ix * k, y + iy * k, k, k, color);
            }
        }
    }
}

/// A filled circle around (cx, cy), with soft edges: a pixel that is half inside
/// gets half the color. e.g. r = 3: a dot 7 pixels across. A `hole` above 0 leaves
/// a hole of that radius in the middle: a ring.
pub(super) fn fill_circle(
    pixels: &mut [u32],
    width: usize,
    cx: f32,
    cy: f32,
    r: f32,
    hole: f32,
    color: u32,
) {
    let height = pixels.len() / width;
    let x0 = (cx - r - 1.0).max(0.0) as usize;
    let y0 = (cy - r - 1.0).max(0.0) as usize;
    let x1 = ((cx + r + 1.0) as usize).min(width);
    let y1 = ((cy + r + 1.0) as usize).min(height);
    for py in y0..y1 {
        for px in x0..x1 {
            // From the middle of the pixel to the middle of the circle.
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            let distance = (dx * dx + dy * dy).sqrt();
            // Half a pixel inside the edge or more: 1, all color. Half a pixel
            // outside or more: 0, none. Right on the edge: 0.5.
            let mut cover = (r + 0.5 - distance).clamp(0.0, 1.0);
            if hole > 0.0 {
                // Minus the hole, the same way.
                cover -= (hole + 0.5 - distance).clamp(0.0, 1.0);
            }
            if cover > 0.0 {
                let pixel = &mut pixels[py * width + px];
                *pixel = blend(*pixel, color, (cover * 255.0) as u32);
            }
        }
    }
}

/// Mixes one color channel: `a` = 0 gives `bg`, 255 gives `fg`.
/// e.g. mix(0, 255, 128) = 128, halfway between.
fn mix(bg: u32, fg: u32, a: u32) -> u32 {
    (fg * a + bg * (255 - a)) / 255
}

/// Blends color `fg` over color `bg`, channel by channel. `a` is 0-255.
/// The top byte too: how clear a color is, with glass (see `glass`). So a
/// letter's edge over glass is part clear, like the glass.
pub(super) fn blend(bg: u32, fg: u32, a: u32) -> u32 {
    let clear = mix(bg >> 24, fg >> 24, a);
    let r = mix((bg >> 16) & 0xff, (fg >> 16) & 0xff, a);
    let g = mix((bg >> 8) & 0xff, (fg >> 8) & 0xff, a);
    let b = mix(bg & 0xff, fg & 0xff, a);
    (clear << 24) | (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_corners() {
        assert_eq!(corner_cover(8, 0, 0), 0);
        assert_eq!(corner_cover(8, 7, 7), 255);
        assert!(corner_cover(8, 0, 7) > 240); // the edge, where the corner ends
        let (w, h) = (20, 20);
        let r = Rect {
            x: 2,
            y: 2,
            w: 16,
            h: 16,
        };
        // Square: as a plain fill.
        let mut pixels = vec![0; w * h];
        fill_round(&mut pixels, w, r, [0; 4], 0xffffff);
        let mut plain = vec![0; w * h];
        fill_rect(&mut plain, w, r.x, r.y, r.w, r.h, 0xffffff);
        assert_eq!(pixels, plain);
        // Round on top only: the top corners cut, the bottom ones filled, the middle solid.
        let mut pixels = vec![0; w * h];
        fill_round(&mut pixels, w, r, [6, 6, 0, 0], 0xffffff);
        assert_eq!(pixels[2 * w + 2], 0);
        assert_eq!(pixels[2 * w + 17], 0);
        assert_eq!(pixels[17 * w + 2], 0xffffff);
        assert_eq!(pixels[17 * w + 17], 0xffffff);
        assert_eq!(pixels[10 * w + 10], 0xffffff);
        // Bigger than the box: as round as it can be, no overflow.
        let mut pixels = vec![0; w * h];
        fill_round(&mut pixels, w, r, [99; 4], 0xffffff);
        assert_eq!(pixels[10 * w + 10], 0xffffff);
    }

    #[test]
    fn blending_over_glass() {
        // A letter (solid) half over glass (51 of 255 clear): half as clear.
        let glass = crate::glass::see_through(0x1e1e1e, 204);
        assert_eq!(blend(glass, 0xd4d4d4, 255), 0x00d4_d4d4);
        assert_eq!(blend(glass, 0xd4d4d4, 0), glass);
        assert_eq!(blend(glass, 0xd4d4d4, 128) >> 24, 25);
        // Without glass, as before: the top byte stays 0.
        assert_eq!(blend(0x1e1e1e, 0xd4d4d4, 128) >> 24, 0);
    }
}
