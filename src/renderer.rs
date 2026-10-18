use crate::term::Grid;
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::window::Window;

const BACKGROUND: u32 = 0x1e1e1e;
const CURSOR: u32 = 0x808080;
const CURSOR_HEIGHT: usize = 4;

// Fixed cell size; FONT_SIZE is picked so a Consolas letter fits in a cell.
const CELL_WIDTH: usize = 10;
const CELL_HEIGHT: usize = 20;
const FONT_SIZE: f32 = 18.0;

pub struct Renderer {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    font: fontdue::Font,
    ascent: i32,
}

impl Renderer {
    pub fn new(window: Rc<Window>) -> anyhow::Result<Self> {
        let context =
            softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let font_bytes = std::fs::read("C:\\Windows\\Fonts\\consola.ttf")?;
        let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let ascent = font
            .horizontal_line_metrics(FONT_SIZE)
            .ok_or_else(|| anyhow::anyhow!("font has no line metrics"))?
            .ascent
            .round() as i32;

        Ok(Renderer {
            window,
            surface,
            font,
            ascent,
        })
    }

    pub fn cell_size(&self) -> (usize, usize) {
        (CELL_WIDTH, CELL_HEIGHT)
    }

    pub fn draw(&mut self, grid: &Grid) {
        let size = self.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        self.surface.resize(w, h).unwrap();
        let mut buffer = self.surface.buffer_mut().unwrap();
        buffer.fill(BACKGROUND);
        let width = size.width as usize;

        let (cursor_row, cursor_col) = grid.cursor();

        for row in 0..grid.rows() {
            let y = row * CELL_HEIGHT;
            for col in 0..grid.cols() {
                let x = col * CELL_WIDTH;

                if (row, col) == (cursor_row, cursor_col) {
                    fill_rect(
                        &mut buffer,
                        width,
                        x,
                        y + CELL_HEIGHT - CURSOR_HEIGHT,
                        CELL_WIDTH,
                        CURSOR_HEIGHT,
                        CURSOR,
                    );
                }

                let cell = grid.cell(row, col);
                if cell.ch != ' ' {
                    draw_char(
                        &mut buffer,
                        width,
                        &self.font,
                        self.ascent,
                        cell.ch,
                        cell.fg,
                        x,
                        y,
                    );
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

/// Draws `ch` in the cell whose top-left corner is at (x, y).
fn draw_char(
    pixels: &mut [u32],
    width: usize,
    font: &fontdue::Font,
    ascent: i32,
    ch: char,
    color: u32,
    x: usize,
    y: usize,
) {
    let (metrics, coverage) = font.rasterize(ch, FONT_SIZE);

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
