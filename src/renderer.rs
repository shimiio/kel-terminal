//! Draws the grid into the window, pixel by pixel, on the CPU.
//!
//! - softbuffer gives us the window's pixels as one long `[u32]` list, row after row.
//!   Pixel (x, y) is at index `y * width + x`. A color is `0xRRGGBB`.
//! - fontdue turns a character into a coverage map: how much of each pixel the
//!   letter covers, 0 (not at all) to 255 (fully).
//! - Every frame redraws everything: backgrounds, letters, cursor.

use crate::app::Selection;
use crate::term::{CursorShape, Grid};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::window::Window;

const BACKGROUND: u32 = 0x1e1e1e;
const SELECTION_BG: u32 = 0x264f78; // blue
const CURSOR: u32 = 0x808080;
const CURSOR_HEIGHT: usize = 4;

/// Consolas comes as 4 files. Index = bold + 2 * italic:
/// 0 = regular, 1 = bold, 2 = italic, 3 = bold italic.
const FONT_FILES: [&str; 4] = [
    "C:\\Windows\\Fonts\\consola.ttf",
    "C:\\Windows\\Fonts\\consolab.ttf",
    "C:\\Windows\\Fonts\\consolai.ttf",
    "C:\\Windows\\Fonts\\consolaz.ttf",
];

/// Font size in pixels at startup. The cell size is measured from it (see `measure_cell`).
pub const FONT_SIZE: f32 = 18.0;
/// Zoom limits: smaller is unreadable, bigger leaves only a few cells in the window.
const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 48.0;

/// A letter already turned into pixels: its metrics and coverage map.
type Glyph = (fontdue::Metrics, Vec<u8>);

pub struct Renderer {
    window: Rc<Window>,
    /// The window's pixels, which we draw into.
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    /// Regular, bold, italic, bold italic (see FONT_FILES).
    fonts: [fontdue::Font; 4],
    font_size: f32,
    /// One cell in pixels.
    cell_w: usize,
    cell_h: usize,
    /// Pixels from the top of a cell down to the baseline, the line letters sit on.
    ascent: i32,
    /// Glyph cache: letters already rasterized, keyed by (letter, font index).
    /// Rasterizing is the slow part of drawing a letter, and for the same letter
    /// and size the result never changes, so each one is rasterized only once.
    /// Cleared when the font size changes.
    glyphs: HashMap<(char, usize), Glyph>,
}

impl Renderer {
    pub fn new(window: Rc<Window>) -> anyhow::Result<Self> {
        let context =
            softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let fonts = [
            load_font(FONT_FILES[0])?,
            load_font(FONT_FILES[1])?,
            load_font(FONT_FILES[2])?,
            load_font(FONT_FILES[3])?,
        ];
        // All 4 files share the same sizes, so measuring the regular one is enough.
        let (cell_w, cell_h, ascent) = measure_cell(&fonts[0], FONT_SIZE)?;

        Ok(Renderer {
            window,
            surface,
            fonts,
            font_size: FONT_SIZE,
            cell_w,
            cell_h,
            ascent,
            glyphs: HashMap::new(),
        })
    }

    pub fn cell_size(&self) -> (usize, usize) {
        (self.cell_w, self.cell_h)
    }

    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Changes the font size and measures the cell again.
    /// The grid doesn't know yet: call `App::on_resize` after this.
    pub fn set_font_size(&mut self, size: f32) {
        let size = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        // Can only fail for a font without line metrics, and `new` already checked that.
        if let Ok((cell_w, cell_h, ascent)) = measure_cell(&self.fonts[0], size) {
            self.font_size = size;
            self.cell_w = cell_w;
            self.cell_h = cell_h;
            self.ascent = ascent;
            // The cached letters have the old size.
            self.glyphs.clear();
        }
    }

    /// Draws one frame. For each cell: 1. background (or selection blue),
    /// 2. the letter on top, 3. the cursor line on top of that.
    pub fn draw(&mut self, grid: &Grid, selection: Option<Selection>, cursor_on: bool) {
        let size = self.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        self.surface.resize(w, h).unwrap();
        let mut buffer = self.surface.buffer_mut().unwrap();
        buffer.fill(BACKGROUND);
        let width = size.width as usize;

        let (cursor_row, cursor_col) = grid.cursor();
        let (cell_w, cell_h) = (self.cell_w, self.cell_h);

        for row in 0..grid.rows() {
            let y = row * cell_h;
            for col in 0..grid.cols() {
                let x = col * cell_w;

                let selected = selection.is_some_and(|s| s.contains(row, col));
                let cell = grid.visible_cell(row, col);
                // Is the cursor drawn on this cell right now?
                let is_cursor = grid.cursor_visible
                    && cursor_on
                    && grid.scroll_offset() == 0
                    && (row, col) == (cursor_row, cursor_col);
                // A block cursor is drawn by swapping colors, so the letter stays visible.
                let block = is_cursor && grid.cursor_shape == CursorShape::Block;

                let (fg, bg) = if cell.inverse {
                    (cell.bg, cell.fg)
                } else {
                    (cell.fg, cell.bg)
                };
                let bg = if selected { SELECTION_BG } else { bg };
                let (fg, bg) = if block { (bg, CURSOR) } else { (fg, bg) };
                fill_rect(&mut buffer, width, x, y, cell_w, cell_h, bg);

                if cell.ch != ' ' {
                    let style = cell.bold as usize + 2 * cell.italic as usize;
                    let font = &self.fonts[style];
                    let size = self.font_size;
                    // Already in the cache: take it. Not yet: rasterize, store, take.
                    let glyph = self
                        .glyphs
                        .entry((cell.ch, style))
                        .or_insert_with(|| font.rasterize(cell.ch, size));
                    draw_glyph(&mut buffer, width, glyph, self.ascent, fg, x, y);
                }

                if cell.underline {
                    fill_rect(&mut buffer, width, x, y + self.ascent as usize + 2, cell_w, 1, fg);
                }

                if is_cursor {
                    let (w, h) = match grid.cursor_shape {
                        CursorShape::Underline => (cell_w, CURSOR_HEIGHT),
                        CursorShape::Bar => (2, cell_h),
                        CursorShape::Block => continue, // already drawn: the swapped colors above
                    };
                    fill_rect(&mut buffer, width, x, y + cell_h - h, w, h, CURSOR);
                }
            }
        }

        buffer.present().unwrap();
    }
}

fn fill_rect(pixels: &mut [u32], width: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    for py in y..y + h {
        for px in x..x + w {
            pixels[py * width + px] = color;
        }
    }
}

fn load_font(path: &str) -> anyhow::Result<fontdue::Font> {
    let bytes = std::fs::read(path)?;
    fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Measures one cell for a font size: (width, height, ascent) in pixels.
/// - width: how far the pen moves after a letter. In a monospaced font it's
///   the same for every letter, so any letter works.
/// - height: one line of text, top of the tallest letter to the bottom of 'g', plus the line gap.
///
/// e.g. Consolas 18: advance 9.90 → 10 wide, line 21.08 → 21 tall,
/// ascent 13.37 + half the gap 1.54 → 15.
fn measure_cell(font: &fontdue::Font, size: f32) -> anyhow::Result<(usize, usize, i32)> {
    let line = font
        .horizontal_line_metrics(size)
        .ok_or_else(|| anyhow::anyhow!("font has no line metrics"))?;
    let cell_w = font.metrics('M', size).advance_width.round() as usize;
    let cell_h = line.new_line_size.round() as usize;
    // The gap is extra space between lines. Half of it goes above the letters,
    // half below, so the text sits in the middle of the cell.
    let ascent = (line.ascent + line.line_gap / 2.0).round() as i32;
    Ok((cell_w, cell_h, ascent))
}

/// Mixes one color channel: `a` = 0 gives `bg`, 255 gives `fg`.
/// e.g. mix(0, 255, 128) = 128, halfway between.
fn mix(bg: u32, fg: u32, a: u32) -> u32 {
    (fg * a + bg * (255 - a)) / 255
}

/// Blends color `fg` over color `bg`, channel by channel. `a` is 0-255.
fn blend(bg: u32, fg: u32, a: u32) -> u32 {
    let r = mix((bg >> 16) & 0xff, (fg >> 16) & 0xff, a);
    let g = mix((bg >> 8) & 0xff, (fg >> 8) & 0xff, a);
    let b = mix(bg & 0xff, fg & 0xff, a);
    (r << 16) | (g << 8) | b
}

/// Draws a rasterized letter in the cell whose top-left corner is at (x, y).
fn draw_glyph(
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
                if window_x < 0 || window_y < 0 {
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
