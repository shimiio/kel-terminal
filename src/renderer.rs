//! Draws the grid into the window, pixel by pixel, on the CPU.
//!
//! - softbuffer gives us the window's pixels as one long `[u32]` list, row after row.
//!   Pixel (x, y) is at index `y * width + x`. A color is `0xRRGGBB`.
//! - fontdue turns a character into a coverage map: how much of each pixel the
//!   letter covers, 0 (not at all) to 255 (fully).
//! - Every frame redraws everything: backgrounds, letters, cursor.

use crate::app::Selection;
use crate::layout::{Layout, Rect, WindowButton};
use crate::term::{CursorShape, Grid};
use crate::theme::Theme;
use anyhow::Context;
use fontdb::{Family, Query, Style, Weight};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::window::Window;

const CURSOR_HEIGHT: usize = 4;

/// One font family in 4 styles. Index = bold + 2 * italic:
/// 0 = regular, 1 = bold, 2 = italic, 3 = bold italic.
pub type Fonts = [fontdue::Font; 4];

/// The size of the text in Kel's own UI (tab names), in pixels.
/// It doesn't follow the terminal's zoom.
const UI_FONT_SIZE: f32 = 14.0;
/// Space between a tab's edge and its name.
const TAB_TEXT_PADDING: usize = 12;
/// The close button under the mouse: the red of Windows' own.
const CLOSE_HOVER: u32 = 0xc42b1c;

/// Zoom limits: smaller is unreadable, bigger leaves only a few cells in the window.
const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 48.0;

/// A letter already turned into pixels: its metrics and coverage map.
type Glyph = (fontdue::Metrics, Vec<u8>);

/// What Kel draws around the terminal: its title bar, with the tabs and the buttons.
pub struct Chrome<'a> {
    /// The tab names, left to right.
    pub titles: &'a [String],
    /// The tab on screen.
    pub active: usize,
    /// The window button under the mouse, drawn lit.
    pub hovered: Option<WindowButton>,
    /// The maximize button then shows "restore" (two squares).
    pub maximized: bool,
    /// Something went wrong (a config error...): shown in red at the end of the tab bar.
    pub status: Option<&'a str>,
}

/// Text in Kel's own UI, like the tab names: one size, its own glyph cache.
struct UiText {
    /// How far each letter moves the pen (the font is monospaced).
    advance: usize,
    /// One line of text, top to bottom.
    line_h: usize,
    ascent: i32,
    glyphs: HashMap<char, Glyph>,
}

impl UiText {
    fn new(font: &fontdue::Font) -> anyhow::Result<UiText> {
        let (advance, line_h, ascent) = measure_cell(font, UI_FONT_SIZE)?;
        Ok(UiText {
            advance,
            line_h,
            ascent,
            glyphs: HashMap::new(),
        })
    }
}

pub struct Renderer {
    window: Rc<Window>,
    /// The window's pixels, which we draw into.
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    fonts: Fonts,
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
    /// Turns the colors in the cells into pixels.
    theme: Theme,
    ui: UiText,
}

impl Renderer {
    pub fn new(
        window: Rc<Window>,
        fonts: Fonts,
        font_size: f32,
        theme: Theme,
    ) -> anyhow::Result<Self> {
        let context =
            softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // All 4 styles share the same sizes, so measuring the regular one is enough.
        let font_size = font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        let (cell_w, cell_h, ascent) = measure_cell(&fonts[0], font_size)?;
        let ui = UiText::new(&fonts[0])?;

        Ok(Renderer {
            window,
            surface,
            fonts,
            font_size,
            cell_w,
            cell_h,
            ascent,
            glyphs: HashMap::new(),
            theme,
            ui,
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

    /// Switches to another font family. Like a size change, the cell is measured
    /// again: call `App::on_resize` after this.
    /// New colors. The cells keep which color they want, so the next frame
    /// shows everything in the new theme, even text that is already there.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn set_fonts(&mut self, fonts: Fonts) {
        if let Ok(ui) = UiText::new(&fonts[0]) {
            self.ui = ui;
        }
        self.fonts = fonts;
        self.glyphs.clear(); // letters of the old font
        self.set_font_size(self.font_size);
    }

    /// Draws one frame. For each cell: 1. background (or selection blue),
    /// 2. the letter on top, 3. the cursor line on top of that.
    pub fn draw(
        &mut self,
        grid: &Grid,
        layout: &Layout,
        chrome: &Chrome,
        selection: Option<Selection>,
        cursor_on: bool,
    ) {
        let size = self.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        self.surface.resize(w, h).unwrap();
        let mut buffer = self.surface.buffer_mut().unwrap();
        let theme = &self.theme;
        let width = size.width as usize;

        // The window around the panel, the panel's border, the panel's background.
        // Each one is painted over the one before, smaller.
        buffer.fill(theme.frame);
        let Layout {
            panel,
            inside,
            text,
            ..
        } = *layout;
        fill_rect(
            &mut buffer,
            width,
            panel.x,
            panel.y,
            panel.w,
            panel.h,
            theme.border,
        );
        fill_rect(
            &mut buffer,
            width,
            inside.x,
            inside.y,
            inside.w,
            inside.h,
            theme.background,
        );
        // After the panel: the tab on screen covers a piece of the panel's top line.
        let font = &self.fonts[0];
        draw_tab_bar(
            &mut buffer,
            width,
            theme,
            layout,
            chrome,
            font,
            &mut self.ui,
        );
        draw_window_buttons(&mut buffer, width, theme, layout, chrome);
        if let Some(status) = chrome.status {
            draw_status(
                &mut buffer,
                width,
                theme,
                layout,
                status,
                font,
                &mut self.ui,
            );
        }

        let (cursor_row, cursor_col) = grid.cursor();
        let (cell_w, cell_h) = (self.cell_w, self.cell_h);

        // Two passes over the grid: first all backgrounds, then all letters on top.
        // A letter can be wider than its cell (Nerd Font icons are ~1.5 cells). In one
        // pass, the next cell's background would be painted over its right part.
        for backgrounds in [true, false] {
            for row in 0..grid.rows() {
                let y = text.y + row * cell_h;
                for col in 0..grid.cols() {
                    let x = text.x + col * cell_w;

                    let selected = selection.is_some_and(|s| s.contains(row, col));
                    let cell = grid.visible_cell(row, col);
                    // Is the cursor drawn on this cell right now?
                    let is_cursor = grid.cursor_visible
                        && cursor_on
                        && grid.scroll_offset() == 0
                        && (row, col) == (cursor_row, cursor_col);
                    // A block cursor is drawn by swapping colors, so the letter stays visible.
                    let block = is_cursor && grid.cursor_shape == CursorShape::Block;

                    // Look the colors up in the theme. "Default" means a different
                    // color for text and background, so this comes before the swap.
                    let fg = theme.rgb(cell.fg, theme.foreground);
                    let bg = theme.rgb(cell.bg, theme.background);
                    let (fg, bg) = if cell.inverse { (bg, fg) } else { (fg, bg) };
                    let bg = if selected { theme.selection } else { bg };
                    let (fg, bg) = if block { (bg, theme.cursor) } else { (fg, bg) };

                    if backgrounds {
                        fill_rect(&mut buffer, width, x, y, cell_w, cell_h, bg);
                        continue;
                    }

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
                        let underline_y = y + self.ascent as usize + 2;
                        fill_rect(&mut buffer, width, x, underline_y, cell_w, 1, fg);
                    }

                    if is_cursor {
                        let (w, h) = match grid.cursor_shape {
                            CursorShape::Underline => (cell_w, CURSOR_HEIGHT),
                            CursorShape::Bar => (2, cell_h),
                            CursorShape::Block => continue, // drawn: the swapped colors above
                        };
                        fill_rect(&mut buffer, width, x, y + cell_h - h, w, h, theme.cursor);
                    }
                }
            }
        }

        buffer.present().unwrap();
    }
}

/// The tab bar. The tab on screen merges into the panel below it: the panel's
/// color inside, the panel's outline around, and no line between them. The other
/// tabs are only their names, dimmer, on the window's color.
fn draw_tab_bar(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    chrome: &Chrome,
    font: &fontdue::Font,
    ui: &mut UiText,
) {
    let Chrome { titles, active, .. } = *chrome;
    // Halfway between the text color and the window's: muted, still readable.
    let dim = blend(theme.frame, theme.foreground, 128);
    let b = layout.border;
    for (i, (tab, title)) in layout.tabs(titles.len()).iter().zip(titles).enumerate() {
        let color = if i == active {
            // The outline, reaching `b` pixels down over the panel's top line…
            fill_rect(pixels, width, tab.x, tab.y, tab.w, tab.h + b, theme.border);
            // …and the inside, in the panel's color, covering that line: one shape.
            let inner_w = tab.w.saturating_sub(2 * b);
            fill_rect(
                pixels,
                width,
                tab.x + b,
                tab.y + b,
                inner_w,
                tab.h,
                theme.background,
            );
            theme.foreground
        } else {
            dim
        };
        // The name, in the middle from top to bottom, cut off where the tab ends.
        let x = tab.x + TAB_TEXT_PADDING;
        let y = tab.y + tab.h.saturating_sub(ui.line_h) / 2;
        let max_w = tab.w.saturating_sub(2 * TAB_TEXT_PADDING);
        for (n, ch) in title.chars().enumerate() {
            let left = x + n * ui.advance;
            if left + ui.advance > x + max_w {
                break;
            }
            let glyph = ui
                .glyphs
                .entry(ch)
                .or_insert_with(|| font.rasterize(ch, UI_FONT_SIZE));
            draw_glyph(pixels, width, glyph, ui.ascent, color, left, y);
        }
    }
}

/// Minimize, maximize and close, top right. Plain icons in the window's colors;
/// the one under the mouse gets a lighter background, close gets red.
fn draw_window_buttons(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    chrome: &Chrome,
) {
    let dim = blend(theme.frame, theme.foreground, 160);
    let lit = blend(theme.frame, theme.foreground, 40);
    for (rect, button) in [
        (layout.minimize, WindowButton::Minimize),
        (layout.maximize, WindowButton::Maximize),
        (layout.close, WindowButton::Close),
    ] {
        let (bg, fg) = match (button, chrome.hovered == Some(button)) {
            (WindowButton::Close, true) => (CLOSE_HOVER, 0xffffff),
            (_, true) => (lit, theme.foreground),
            (_, false) => (theme.frame, dim),
        };
        fill_rect(pixels, width, rect.x, rect.y, rect.w, rect.h, bg);

        // The icon: 10x10 pixels in the middle of the button.
        let (x, y) = (
            (rect.x + rect.w / 2).saturating_sub(5),
            (rect.y + rect.h / 2).saturating_sub(5),
        );
        match button {
            WindowButton::Minimize => fill_rect(pixels, width, x, y + 5, 10, 1, fg),
            WindowButton::Maximize if chrome.maximized => {
                // "Restore": a square, and the corner of another one behind it.
                outline(
                    pixels,
                    width,
                    Rect {
                        x,
                        y: y + 2,
                        w: 8,
                        h: 8,
                    },
                    fg,
                );
                fill_rect(pixels, width, x + 2, y, 8, 1, fg);
                fill_rect(pixels, width, x + 9, y, 1, 8, fg);
            }
            WindowButton::Maximize => outline(pixels, width, Rect { x, y, w: 10, h: 10 }, fg),
            WindowButton::Close => {
                // An X: two diagonals, one pixel per row.
                for i in 0..10 {
                    fill_rect(pixels, width, x + i, y + i, 1, 1, fg);
                    fill_rect(pixels, width, x + 9 - i, y + i, 1, 1, fg);
                }
            }
        }
    }
}

/// A message in red at the right end of the tab bar, just before the window buttons.
/// It goes over the tabs if they reach that far: a problem should be seen.
fn draw_status(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    status: &str,
    font: &fontdue::Font,
    ui: &mut UiText,
) {
    let bar = layout.tab_bar;
    // At most the right half of the bar; a longer message is cut.
    let max_chars = (bar.w / 2) / ui.advance.max(1);
    let shown: String = status.chars().take(max_chars).collect();
    let text_w = shown.chars().count() * ui.advance;
    let end = bar.x + bar.w;
    let x = end.saturating_sub(text_w);
    // The window's color behind it, so tabs under it don't show through.
    let back = x.saturating_sub(TAB_TEXT_PADDING);
    fill_rect(pixels, width, back, bar.y, end - back, bar.h, theme.frame);
    let y = bar.y + bar.h.saturating_sub(ui.line_h) / 2;
    for (n, ch) in shown.chars().enumerate() {
        let glyph = ui
            .glyphs
            .entry(ch)
            .or_insert_with(|| font.rasterize(ch, UI_FONT_SIZE));
        draw_glyph(
            pixels,
            width,
            glyph,
            ui.ascent,
            theme.palette[9],
            x + n * ui.advance,
            y,
        );
    }
}

/// A rectangle's outline, one pixel thick.
fn outline(pixels: &mut [u32], width: usize, r: Rect, color: u32) {
    fill_rect(pixels, width, r.x, r.y, r.w, 1, color); // top
    fill_rect(pixels, width, r.x, r.y + r.h - 1, r.w, 1, color); // bottom
    fill_rect(pixels, width, r.x, r.y, 1, r.h, color); // left
    fill_rect(pixels, width, r.x + r.w - 1, r.y, 1, r.h, color); // right
}

fn fill_rect(pixels: &mut [u32], width: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    // Only the part inside the window: in a very small window the panel or the
    // grid can stick out, and a pixel past the edge would land on the next row.
    let height = pixels.len() / width;
    for py in y..(y + h).min(height) {
        for px in x..(x + w).min(width) {
            pixels[py * width + px] = color;
        }
    }
}

/// Finds the font family `family` (e.g. "Consolas") among the installed fonts and
/// loads its 4 styles. A family without, say, an italic file gets the closest style.
pub fn load_fonts(family: &str) -> anyhow::Result<Fonts> {
    let mut db = fontdb::Database::new();
    // Reads the names of all installed fonts, in C:\Windows\Fonts and the user's
    // own fonts folder. About 20 ms (the first time after a reboot, ~1 s: from disk).
    db.load_system_fonts();

    let load = |weight: Weight, style: Style| -> anyhow::Result<fontdue::Font> {
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
        db.with_face_data(id, |data, index| {
            let settings = fontdue::FontSettings {
                collection_index: index,
                ..Default::default()
            };
            fontdue::Font::from_bytes(data, settings).map_err(|e| anyhow::anyhow!("{e}"))
        })
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
