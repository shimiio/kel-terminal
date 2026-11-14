//! Letters: the terminal's glyphs and their cache, the UI's text, underlines,
//! and how big a cell is.

use super::shapes::{blend, fill_rect};
use super::{Glyph, Ui};
use crate::fallback::Fallbacks;
use crate::font::Font;
use crate::layout::scaled;
use crate::term::{Underline, char_width};
use std::collections::HashMap;

/// `text` in at most `chars` letters, cut at the start: a folder's end says the
/// most. e.g. "…\Documents\notes" in 12: "…ments\notes".
pub(super) fn keep_end(text: &str, chars: usize) -> String {
    let count = text.chars().count();
    if count <= chars {
        return text.to_string();
    }
    if chars == 0 {
        return String::new();
    }
    let tail: String = text.chars().skip(count - (chars - 1)).collect();
    format!("…{tail}")
}

/// One line of UI text from (x, y), cut off where it would pass `max_w` pixels.
/// Returns how wide the drawn part is.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_ui_text(
    pixels: &mut [u32],
    width: usize,
    font: &Font,
    ui: &mut Ui,
    text: &str,
    (x, y): (usize, usize),
    max_w: usize,
    color: u32,
) -> usize {
    let size = ui.font_size();
    let mut drawn = 0;
    for ch in text.chars() {
        // A wide one (emoji, Chinese) takes two letters' room.
        let room = char_width(ch).max(1) * ui.advance;
        if drawn + room > max_w {
            break;
        }
        let Ui {
            glyphs,
            fallbacks,
            ascent,
            ..
        } = &mut *ui;
        let glyph = glyphs
            .entry(ch)
            .or_insert_with(|| ui_glyph(font, fallbacks, ch, size, room));
        draw_glyph(pixels, width, glyph, *ascent, color, x + drawn, y);
        drawn += room;
    }
    drawn
}

/// The UI font's letter `ch`; one it doesn't have from a fallback font (see
/// `fallback`), made smaller if it's wider than its `room`. Without one: the
/// font's empty box.
fn ui_glyph(font: &Font, fallbacks: &mut Fallbacks, ch: char, size: f32, room: usize) -> Glyph {
    if font.has_glyph(ch) {
        return font.rasterize(ch, size);
    }
    let Some(fallback) = fallbacks.font_for(ch) else {
        return font.rasterize(ch, size);
    };
    let advance = fallback.advance(ch, size);
    let size = match advance > room as f32 {
        true => size * room as f32 / advance,
        false => size,
    };
    fallback.rasterize(ch, size)
}

/// A hint, what to press or what a thing does, in italics: it reads as a
/// side note, apart from the names and commands around it. Cut off at `max_w`
/// like `draw_ui_text`; the italic has the same widths, so they line up.
pub(super) fn draw_ui_hint(
    pixels: &mut [u32],
    width: usize,
    ui: &mut Ui,
    text: &str,
    (x, y): (usize, usize),
    max_w: usize,
    color: u32,
) {
    let size = ui.font_size();
    let mut drawn = 0;
    for ch in text.chars() {
        if drawn + ui.advance > max_w {
            break;
        }
        let italic = &ui.italic;
        let glyph = ui
            .italic_glyphs
            .entry(ch)
            .or_insert_with(|| italic.rasterize(ch, size));
        draw_glyph(pixels, width, glyph, ui.ascent, color, x + drawn, y);
        drawn += ui.advance;
    }
}

/// `text` in at most `max` characters: the middle goes, for a `…`. e.g.
/// `https://example.com/a/b/c/page` in 20: `https:/…m/a/b/c/page`.
pub(super) fn cut_middle(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max || max < 3 {
        return text.to_string();
    }
    let head = (max - 1) * 2 / 5;
    let tail = max - 1 - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(n - tail).collect();
    format!("{start}…{end}")
}

/// The glyph of `c` in font `style`, from `cache`: rasterized the first time,
/// which is the slow part of drawing a letter; for the same letter and size the
/// result never changes.
///
/// A letter the font lacks comes from a fallback font, made smaller if it's
/// bigger than its cells (1, or 2 for a wide one): Segoe UI Emoji's 😀 is 2.5
/// cells wide at the font's size. The built-in icons may take 2: they're drawn
/// big and reach into the next cell (a space, after an icon), as in Nerd Fonts'
/// own fonts. A letter no font has: the font's "missing" box.
pub(super) fn glyph<'a>(
    cache: &'a mut HashMap<(char, usize), Glyph>,
    font: &Font,
    fallbacks: &mut Fallbacks,
    (c, style): (char, usize),
    size: f32,
    cell_w: usize,
) -> &'a Glyph {
    cache.entry((c, style)).or_insert_with(|| {
        if font.has_glyph(c) {
            return font.rasterize(c, size);
        }
        let is_icon = fallbacks.is_icon(c);
        let Some(fallback) = fallbacks.font_for(c) else {
            return font.rasterize(c, size);
        };
        let cells = if is_icon { 2 } else { char_width(c) };
        let room = (cells * cell_w) as f32;
        let advance = fallback.advance(c, size);
        let size = if advance > room {
            size * room / advance
        } else {
            size
        };
        fallback.rasterize(c, size)
    })
}

/// Draws the underline of a cell at `(x, y)`, `w` x `h`, below the baseline
/// (`baseline` pixels below the cell's top). Lines grow with the text (`scale`).
///
/// Neighbors must join into one line: the dots and dashes count from the
/// window's left edge, not the cell's, and the curly one is one whole wave per
/// cell, so it ends where the next one starts.
pub(super) fn draw_underline(
    pixels: &mut [u32],
    width: usize,
    style: Underline,
    (x, y, w, h): (usize, usize, usize, usize),
    baseline: usize,
    scale: f32,
    color: u32,
) {
    let t = scaled(1, scale); // a line's thickness
    // The tallest kinds (double, curly) need 4 lines; a font with little room
    // below the baseline still gets them inside the cell.
    let low = (y + h).saturating_sub(4 * t);
    let line_y = (y + baseline + scaled(2, scale)).min(low + 3 * t);
    let band_y = (y + baseline + scaled(1, scale)).min(low);
    match style {
        Underline::None => {}
        Underline::Single => fill_rect(pixels, width, x, line_y, w, t, color),
        Underline::Double => {
            fill_rect(pixels, width, x, band_y, w, t, color);
            fill_rect(pixels, width, x, band_y + 2 * t, w, t, color);
        }
        // On 2 pixels (at 100%), off 2.
        Underline::Dotted => {
            for px in (x..x + w).filter(|px| (px / (2 * t)).is_multiple_of(2)) {
                fill_rect(pixels, width, px, line_y, 1, t, color);
            }
        }
        // On 6, off 3.
        Underline::Dashed => {
            for px in (x..x + w).filter(|px| (px / (3 * t)) % 3 != 2) {
                fill_rect(pixels, width, px, line_y, 1, t, color);
            }
        }
        // Down from the top of the band and back up, 3 lines deep (at 100%):
        // (1 - cos) / 2 goes 0 → 1 → 0 over one cell. Each pixel column also
        // fills the gap to the one before, so the steep parts don't break up.
        Underline::Curly => {
            let depth = (3 * t) as f32;
            let at = |px: usize| {
                let turn = px as f32 / w as f32 * std::f32::consts::TAU;
                ((1.0 - turn.cos()) / 2.0 * depth).round() as usize
            };
            let mut before = 0;
            for px in 0..w {
                let now = at(px);
                let (top, bottom) = (before.min(now), before.max(now));
                fill_rect(
                    pixels,
                    width,
                    x + px,
                    band_y + top,
                    1,
                    bottom - top + t,
                    color,
                );
                before = now;
            }
        }
    }
}

/// One cell's size, measured (see `measure_cell`).
#[derive(Clone, Copy)]
pub(super) struct Measured {
    pub(super) w: usize,
    pub(super) h: usize,
    /// Pixels from the cell's top down to the baseline.
    pub(super) ascent: i32,
    /// The font's own line, without `line_height`: what a letter is drawn for.
    /// A line or a block (`│`, `█`) is stretched from it to `h` (see `fills_cell`).
    font_h: usize,
    font_ascent: i32,
}

/// Measures one cell for a font size: (width, height, ascent) in pixels.
/// - width: how far the pen moves after a letter. In a monospaced font it's
///   the same for every letter, so any letter works.
/// - height: one line of text, top of the tallest letter to the bottom of 'g', plus the line gap.
///
/// e.g. Consolas 18: advance 9.90 → 10 wide, line 21.08 → 21 tall,
/// ascent 13.37 + half the gap 1.54 → 15.
/// `line_height`: the cell that many times the font's line tall, the letters
/// in its middle (1: as the font says).
pub(super) fn measure_cell(font: &Font, size: f32, line_height: f32) -> anyhow::Result<Measured> {
    let line = font.line_metrics(size);
    let cell_w = font.advance('M', size).round() as usize;
    let font_h = line.new_line_size.round() as usize;
    if cell_w == 0 || font_h == 0 {
        anyhow::bail!("the font has no size (no 'M', or no line height)");
    }
    // The gap is extra space between lines. Half of it goes above the letters,
    // half below, so the text sits in the middle of the cell.
    let font_ascent = (line.ascent + line.line_gap / 2.0).round() as i32;
    // So does `line_height`'s room: half above, half below.
    let cell_h = (line.new_line_size * line_height).round().max(1.0) as usize;
    let room = cell_h as i32 - font_h as i32;
    Ok(Measured {
        w: cell_w,
        h: cell_h,
        ascent: font_ascent + room / 2,
        font_h,
        font_ascent,
    })
}

/// Letters that fill their cell to its edges, so the next row's go on from
/// them: box lines (`│ ─ ┌`, lazygit's frames), blocks (`█ ▀ ▄`), powerline's
/// arrows (nvim's status lines). Drawn by `draw_stretched`: the font's edges are
/// soft (a thin dark seam between two `█`), and with `line_height` above 1 it
/// draws them shorter than the cell (a frame with gaps).
pub(super) fn fills_cell(ch: char) -> bool {
    matches!(ch, '\u{2500}'..='\u{259F}' | '\u{E0B0}'..='\u{E0BF}')
}

/// Draws a letter that fills its cell (see `fills_cell`) to the cell's edges.
/// Its middle as the font draws it (a `─` stays thin), its edge rows, half
/// covered, are their neighbors inside instead (no seam); and a cell taller than
/// the font's line (`line_height`) repeats them up and down, so `│` goes on.
pub(super) fn draw_stretched(
    pixels: &mut [u32],
    width: usize,
    glyph: &Glyph,
    m: Measured,
    color: u32,
    (x, y, cell_h): (usize, usize, usize),
) {
    let (metrics, coverage) = glyph;
    // Where the letter is in the font's line: its top row there.
    let top = m.font_ascent - (metrics.height as i32 + metrics.ymin);
    let left = x as i32 + metrics.xmin;
    // The font's line sits in the middle of the cell (see `measure_cell`).
    let above = (cell_h as i32 - m.font_h as i32) / 2;
    let last = (m.font_h as i32 - 2).max(1);
    for dy in 0..cell_h {
        // The row of the font's line this one shows: past an edge, the edge;
        // an edge row, the one inside it.
        let fy = (dy as i32 - above).clamp(1, last);
        let gy = fy - top;
        if gy < 0 || gy >= metrics.height as i32 {
            continue;
        }
        let row = &coverage[gy as usize * metrics.width..][..metrics.width];
        for (gx, &value) in row.iter().enumerate() {
            let window_x = left + gx as i32;
            if value == 0 || window_x < 0 || window_x as usize >= width {
                continue;
            }
            let i = (y + dy) * width + window_x as usize;
            if let Some(px) = pixels.get_mut(i) {
                *px = blend(*px, color, value as u32);
            }
        }
    }
}

/// Draws a rasterized letter in the cell whose top-left corner is at (x, y).
pub(super) fn draw_glyph(
    pixels: &mut [u32],
    width: usize,
    glyph: &Glyph,
    ascent: i32,
    color: u32,
    x: usize,
    y: usize,
) {
    let (metrics, coverage) = glyph;

    // Letters sit on the baseline, `ascent` pixels below the top of the cell.
    // ymin is how far the letter's bottom is below the baseline (negative for 'g', 'p'...).
    let top = y as i32 + ascent - (metrics.height as i32 + metrics.ymin);
    let left = x as i32 + metrics.xmin;

    for gy in 0..metrics.height {
        for gx in 0..metrics.width {
            let value = coverage[gy * metrics.width + gx];
            if value > 0 {
                let window_x = left + gx as i32;
                let window_y = top + gy as i32;
                // Off the window. Past the right edge matters too: pixel x = width
                // would land at the start of the next row.
                if window_x < 0 || window_y < 0 || window_x as usize >= width {
                    continue;
                }
                let i = window_y as usize * width + window_x as usize;
                if i >= pixels.len() {
                    continue;
                }

                pixels[i] = blend(pixels[i], color, value as u32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font;

    #[test]
    fn a_taller_line_keeps_the_letters_in_its_middle() {
        let [font, ..] = font::built_in();
        let plain = measure_cell(&font, 20.0, 1.0).unwrap();
        let tall = measure_cell(&font, 20.0, 1.5).unwrap();
        assert_eq!(plain.h, plain.font_h);
        assert_eq!(tall.w, plain.w); // only taller
        assert_eq!(tall.font_h, plain.font_h);
        // About half again: each is rounded from the font's line (26.4 → 26, 39.6 → 40).
        assert!(
            (tall.h as f32 / plain.h as f32 - 1.5).abs() < 0.1,
            "{}",
            tall.h
        );
        // Half the room above the letters: the baseline moves down by it.
        assert_eq!(tall.ascent, plain.ascent + (tall.h - plain.h) as i32 / 2);
        // Frames and blocks stretch; letters don't.
        assert!(fills_cell('│') && fills_cell('█') && fills_cell('\u{E0B0}'));
        assert!(!fills_cell('a') && !fills_cell('|'));
    }

    #[test]
    fn a_long_address_loses_its_middle() {
        let url = "https://example.com/a/b/c/page";
        assert_eq!(cut_middle(url, 20), "https:/…m/a/b/c/page");
        assert_eq!(cut_middle(url, 20).chars().count(), 20);
        assert_eq!(cut_middle("short", 20), "short");
    }

    #[test]
    fn a_long_hint_keeps_its_end() {
        assert_eq!(keep_end(r"C:\code\web", 20), r"C:\code\web");
        assert_eq!(keep_end(r"…\Documents\notes", 12), r"…ments\notes");
        assert_eq!(keep_end("abc", 0), "");
    }
}
