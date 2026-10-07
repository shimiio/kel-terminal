use crate::term::Grid;
use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::window::Window;

const FONT_PATH: &str = "C:\\Windows\\Fonts\\consola.ttf";
const FONT_SIZE: f32 = 22.0; // in logical pixels, scaled by the monitor's DPI

const BACKGROUND: u32 = 0x1e1e1e;
const FOREGROUND: u32 = 0xd4d4d4;

/// A rasterized character: its placement info and an alpha value (0-255) per pixel.
struct Glyph {
    metrics: Metrics,
    coverage: Vec<u8>,
}

pub struct Renderer {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    font: Font,
    font_px: f32,
    cell_width: usize,
    cell_height: usize,
    /// Distance from the top of a cell down to the text baseline.
    ascent: usize,
    // Rasterizing is slow, so each char is rasterized once and reused.
    glyphs: HashMap<char, Glyph>,
}

impl Renderer {
    pub fn new(window: Rc<Window>) -> anyhow::Result<Self> {
        let context =
            softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let font = Font::from_bytes(std::fs::read(FONT_PATH)?, FontSettings::default())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let font_px = FONT_SIZE * window.scale_factor() as f32;
        let line = font
            .horizontal_line_metrics(font_px)
            .ok_or_else(|| anyhow::anyhow!("font has no horizontal metrics"))?;
        // Monospace font: every char is as wide as 'M'.
        let cell_width = font.metrics('M', font_px).advance_width.ceil() as usize;
        let cell_height = line.new_line_size.ceil() as usize;

        Ok(Renderer {
            window,
            surface,
            font,
            font_px,
            cell_width,
            cell_height,
            ascent: line.ascent.ceil() as usize,
            glyphs: HashMap::new(),
        })
    }

    /// Size of one grid cell in physical pixels, as (width, height).
    pub fn cell_size(&self) -> (usize, usize) {
        (self.cell_width, self.cell_height)
    }

    pub fn draw(&mut self, grid: &Grid) {
        let size = self.window.inner_size(); // window size in pixels
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        self.surface.resize(w, h).unwrap(); // make the pixel buffer match the window
        let mut buffer = self.surface.buffer_mut().unwrap(); // get the pixel array
        buffer.fill(BACKGROUND);

        let mut canvas = Canvas {
            pixels: &mut buffer,
            width: size.width as usize,
            height: size.height as usize,
        };

        let (cursor_row, cursor_col) = grid.cursor();
        for row in 0..grid.rows() {
            for col in 0..grid.cols() {
                let x = col * self.cell_width;
                let y = row * self.cell_height;

                // DEBUG chess grid
                if (row + col) % 2 == 0 {
                    canvas.fill_rect(x, y, self.cell_width, self.cell_height, 0x2a2a2a);
                }
                canvas.fill_rect(x, y + self.ascent, self.cell_width, 1, 0x4a4a4a);

                let is_cursor = (row, col) == (cursor_row, cursor_col);

                // Block cursor: fill the cell, then draw its char in the inverted color.
                let fg = if is_cursor {
                    canvas.fill_rect(x, y, self.cell_width, self.cell_height, FOREGROUND);
                    BACKGROUND
                } else {
                    FOREGROUND
                };

                let ch = grid.cell(row, col).ch;
                if ch == ' ' {
                    continue;
                }
                let glyph = self
                    .glyphs
                    .entry(ch)
                    .or_insert_with(|| rasterize(&self.font, ch, self.font_px));
                canvas.draw_glyph(glyph, x, y + self.ascent, fg);
            }
        }

        buffer.present().unwrap(); // show it on screen
    }
}

fn rasterize(font: &Font, ch: char, px: f32) -> Glyph {
    let (metrics, coverage) = font.rasterize(ch, px);
    Glyph { metrics, coverage }
}

/// The pixel buffer plus its size, with helpers that clip at the edges.
struct Canvas<'a> {
    pixels: &'a mut [u32],
    width: usize,
    height: usize,
}

impl Canvas<'_> {
    fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        for py in y..(y + h).min(self.height) {
            let start = py * self.width;
            self.pixels[start + x.min(self.width)..start + (x + w).min(self.width)].fill(color);
        }
    }

    /// Draws `glyph` with its baseline at `baseline_y`, starting at column `x`.
    fn draw_glyph(&mut self, glyph: &Glyph, x: usize, baseline_y: usize, color: u32) {
        let m = &glyph.metrics;
        // ymin is how far the bitmap's bottom sits below the baseline (negative for 'g', 'p'...).
        let top = baseline_y as i32 - (m.height as i32 + m.ymin);
        let left = x as i32 + m.xmin;

        for gy in 0..m.height {
            for gx in 0..m.width {
                let alpha = glyph.coverage[gy * m.width + gx];
                if alpha == 0 {
                    continue;
                }
                let (px, py) = (left + gx as i32, top + gy as i32);
                if px < 0 || py < 0 || px as usize >= self.width || py as usize >= self.height {
                    continue;
                }
                let i = py as usize * self.width + px as usize;
                self.pixels[i] = blend(self.pixels[i], color, alpha);
            }
        }
    }
}

/// Mixes `fg` over `bg`: alpha 0 keeps `bg`, 255 gives `fg`.
/// Colors are 0x00RRGGBB, so each channel is blended separately.
fn blend(bg: u32, fg: u32, alpha: u8) -> u32 {
    let a = alpha as u32;
    let channel = |shift: u32| {
        let b = (bg >> shift) & 0xff;
        let f = (fg >> shift) & 0xff;
        ((f * a + b * (255 - a)) / 255) << shift
    };
    channel(16) | channel(8) | channel(0)
}
