//! Draws the grid into the window, pixel by pixel, on the CPU.
//!
//! - softbuffer gives us the window's pixels as one long `[u32]` list, row after row.
//!   Pixel (x, y) is at index `y * width + x`. A color is `0xRRGGBB`.
//! - `font` turns a character into a coverage map: how much of each pixel the
//!   letter covers, 0 (not at all) to 255 (fully).
//! - Every frame redraws everything: backgrounds, letters, cursor.

use crate::app::Selection;
use crate::backdrop::Backdrop;
use crate::card::{self, Card};
use crate::fallback::Fallbacks;
use crate::font::{self, Font, Fonts, Metrics};
use crate::glass::Glass;
use crate::help::{Help, Line};
use crate::icons::{Icon, Icons};
use crate::layout::{Layout, Rect, TitleHit, WindowButton, scaled};
use crate::logo;
use crate::palette::Palette;
use crate::search::{self, Search};
use crate::sidebar::{self, Entry, Live, Row, Sidebar};
use crate::start::{self, Start};
use crate::term::Found;
use crate::term::{CursorShape, Grid, Underline, Width, char_width};
use crate::theme::Theme;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::window::Window;

const CURSOR_HEIGHT: usize = 4;

/// The size of the text in Kel's own UI (tab names), in pixels at 100%.
/// It doesn't follow the terminal's zoom. Sizes here are for 100% too; the UI's
/// scale multiplies them (see `Ui::px`).
const UI_FONT_SIZE: f32 = 14.0;
/// Space between a tab's edge and its icon.
const TAB_TEXT_PADDING: usize = 12;
/// Space between a tab's icon and its name.
const ICON_GAP: usize = 8;
/// The mark on a tab (activity dot, busy ring): its radius, in pixels.
const DOT_RADIUS: f32 = 3.0;
/// The busy ring's line.
const RING_WIDTH: f32 = 1.5;
/// The close button under the mouse: the red of Windows' own.
const CLOSE_HOVER: u32 = 0xc42b1c;

/// The project's name before the tabs: from this far into the tab bar, and this
/// far from the first tab. A longer name than `PROJECT_NAME_CHARS` letters is cut.
const PROJECT_NAME_LEFT: usize = 4;
const PROJECT_NAME_GAP: usize = 14;
const PROJECT_NAME_CHARS: usize = 16;

/// How dark the window gets behind the open sidebar: this much black (of 255).
const SIDEBAR_DIM: u32 = 120;

/// The search box: this wide, in letters (less in a narrow window).
const SEARCH_CHARS: usize = 36;
/// How much of the dim yellow the other matches get over their background (of 255).
const MATCH_TINT: u32 = 100;
/// How much of the text's color lights the panel when the bell flashes, out of 255.
const FLASH: u32 = 40;

/// The command palette: at most this wide (in letters) and this many lines.
const PALETTE_CHARS: usize = 64;
const PALETTE_LINES: usize = 10;
/// Space above and below a palette line's text, and at its sides, at 100%.
const PALETTE_PADDING: usize = 6;

/// Zoom limits: smaller is unreadable, bigger leaves only a few cells in the window.
const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 48.0;

/// A letter already turned into pixels: its metrics and coverage map.
type Glyph = (Metrics, Vec<u8>);

/// One tab, as the tab bar shows it.
pub struct TabLabel<'a> {
    pub title: &'a str,
    /// The icon's name, see `icons::for_title`.
    pub icon: &'a str,
    /// New output since the tab was last on screen: a dot.
    pub activity: bool,
    /// A command runs: a ring.
    pub busy: bool,
    /// A command ended unseen: a green dot if it worked, red if not.
    pub done: Option<bool>,
}

/// What Kel draws around the terminal: its title bar, with the tabs and the buttons.
pub struct Chrome<'a> {
    /// The tabs, left to right.
    pub tabs: &'a [TabLabel<'a>],
    /// The tab on screen.
    pub active: usize,
    /// The window button under the mouse, drawn lit.
    pub hovered: Option<TitleHit>,
    /// The maximize button then shows "restore" (two squares).
    pub maximized: bool,
    /// Something went wrong (a config error...): shown in red at the end of the tab bar.
    pub status: Option<&'a str>,
    /// The address of the link under the mouse, if a program marked the text as
    /// one (OSC 8): it isn't on the screen. Where the status goes, dim, when
    /// there's no status or question.
    pub link_url: Option<&'a str>,
    /// A question ("close again to quit"): in its place, in yellow, while it waits.
    pub question: Option<&'a str>,
    /// The command palette, if it's open: drawn over everything else.
    pub palette: Option<&'a Palette>,
    /// The search in the output, if it's open: its box, and its matches marked.
    pub search: Option<&'a Search>,
    /// The open project: its name, before the tabs, and its own color, if it has
    /// one. That color is the window's accent then (see `Theme::accent_for`).
    pub project: Option<(&'a str, Option<u32>)>,
    /// The link under the mouse while Ctrl is held: underlined. Lasting rows.
    pub link: Option<Found>,
    /// The sidebar, if it's open: over the left of the window, the rest dimmed.
    pub sidebar: Option<&'a Sidebar>,
    /// The start screen, if it shows: over the terminal.
    pub start: Option<&'a Start>,
    /// The open card, and where it is.
    pub card: Option<(&'a Card, Rect)>,
    /// Whether the keys go to the terminal. Not while the sidebar, the palette,
    /// the search, a card or the help has them, nor in another window: its
    /// cursor is a hollow box then, still, where typing will go on.
    pub keys_here: bool,
    /// The terminal's background solid, not glass: a full-screen program (lazygit)
    /// or Claude Code is on screen. They leave the background to the terminal,
    /// and their many colors and lines read worse over what's behind the window.
    /// nvim paints its own, so there it changes nothing.
    pub solid: bool,
    /// The bell just rang (`bell = "flash"`): the terminal a bit lighter.
    pub flash: bool,
    /// The help, if it's open: over everything.
    pub help: Option<&'a Help>,
}

/// Kel's own UI: the tab names (one text size, its own glyph cache) and the icons.
struct Ui {
    /// The UI's scale, the same as the layout's.
    scale: f32,
    /// The icons' pixel art only grows by whole steps (see `set_scale`).
    icon_scale: usize,
    /// How far each letter moves the pen (the font is monospaced).
    advance: usize,
    /// One line of text, top to bottom.
    line_h: usize,
    ascent: i32,
    glyphs: HashMap<char, Glyph>,
    /// For what the UI's font doesn't have: a program's title can hold anything,
    /// e.g. Claude Code's spinning `✳ ✶ ✻` (see `ui_glyph`).
    fallbacks: Fallbacks,
    /// The hints' font: the built-in italic (see `draw_ui_hint`), and its letters.
    italic: Font,
    italic_glyphs: HashMap<char, Glyph>,
    icons: Icons,
    /// The project's name in the pixel font (`pixel_name` in the config), or
    /// in this font.
    pixel_name: bool,
}

impl Ui {
    fn new(font: &Font, italic: Font) -> anyhow::Result<Ui> {
        let mut ui = Ui {
            scale: 1.0,
            icon_scale: 1,
            advance: 0,
            line_h: 0,
            ascent: 0,
            glyphs: HashMap::new(),
            fallbacks: Fallbacks::default(),
            italic,
            italic_glyphs: HashMap::new(),
            icons: Icons::load(),
            pixel_name: true,
        };
        ui.set_font(font)?;
        Ok(ui)
    }

    /// Measures the text for another font. The letters of the old one go.
    fn set_font(&mut self, font: &Font) -> anyhow::Result<()> {
        let m = measure_cell(font, self.font_size(), 1.0)?;
        (self.advance, self.line_h, self.ascent) = (m.w, m.h, m.ascent);
        self.glyphs.clear();
        self.italic_glyphs.clear();
        Ok(())
    }

    /// A new scale: the text is measured again at its new size.
    fn set_scale(&mut self, font: &Font, scale: f32) -> anyhow::Result<()> {
        self.scale = scale;
        // Pixel art gets uneven at 1.5: of 16 pixels, every other one would be
        // 2 wide and the rest 1. So whole steps only, rounded down: ×1 up to
        // 199%, ×2 from 200%, ×3 from 300%.
        self.icon_scale = (scale.floor() as usize).max(1);
        self.set_font(font)
    }

    /// The UI text's size in pixels, e.g. 14 at 100%, 21 at 150%.
    fn font_size(&self) -> f32 {
        UI_FONT_SIZE * self.scale
    }

    /// `n` pixels at 100%, at this scale.
    fn px(&self, n: usize) -> usize {
        scaled(n, self.scale)
    }
}

pub struct Renderer {
    window: Rc<Window>,
    /// The window's pixels, which we draw into.
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    fonts: Fonts,
    /// Kel's own UI's font: the built-in one, whatever the terminal's is.
    ui_font: Font,
    /// The terminal's font size at 100%: the config's, or zoomed.
    font_size: f32,
    /// Windows' display scale, e.g. 1.5 at 150%. The font is drawn
    /// `font_size * text_scale` pixels big, so it looks the same size on any display.
    text_scale: f32,
    /// One cell in pixels.
    cell_w: usize,
    cell_h: usize,
    /// Pixels from the top of a cell down to the baseline, the line letters sit on.
    ascent: i32,
    /// The cell this many times the font's line tall (`line_height` in the config).
    line_height: f32,
    /// The cell as measured, with the font's own line (see `Measured`).
    measured: Measured,
    /// Glyph cache: letters already rasterized, keyed by (letter, font index).
    /// Rasterizing is the slow part of drawing a letter, and for the same letter
    /// and size the result never changes, so each one is rasterized only once.
    /// Cleared when the font size changes.
    glyphs: HashMap<(char, usize), Glyph>,
    /// For letters the font doesn't have: emoji, Chinese...
    fallbacks: Fallbacks,
    /// The config's theme, as it is: `theme` is made from it.
    plain_theme: Theme,
    /// `theme` with a solid background, for under a full-screen program (see
    /// `Chrome::solid`); the frame stays glass.
    solid_theme: Theme,
    /// See-through or not (see `glass`).
    glass: Glass,
    /// The cursor where the program hasn't asked for one (`cursor` in the config).
    cursor_shape: CursorShape,
    /// Turns the colors in the cells into pixels. With glass, its background
    /// and frame are see-through (`Glass::theme`).
    theme: Theme,
    ui: Ui,
}

impl Renderer {
    pub fn new(
        window: Rc<Window>,
        fonts: Fonts,
        font_size: f32,
        text_scale: f32,
        theme: Theme,
        glass: Glass,
    ) -> anyhow::Result<Self> {
        let context =
            softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // All 4 styles share the same sizes, so measuring the regular one is enough.
        let font_size = font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        let measured = measure_cell(&fonts[0], font_size * text_scale, 1.0)?;
        let [ui_font, _, italic, _] = font::built_in();
        let ui = Ui::new(&ui_font, italic)?;

        Ok(Renderer {
            window,
            surface,
            fonts,
            ui_font,
            font_size,
            text_scale,
            cell_w: measured.w,
            cell_h: measured.h,
            ascent: measured.ascent,
            line_height: 1.0,
            measured,
            glyphs: HashMap::new(),
            fallbacks: Fallbacks::default(),
            theme: glass.theme(&theme),
            solid_theme: solid(&glass, &theme),
            plain_theme: theme,
            glass,
            cursor_shape: CursorShape::default(),
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
        let font = &self.fonts[0];
        if let Ok(m) = measure_cell(font, size * self.text_scale, self.line_height) {
            self.font_size = size;
            (self.cell_w, self.cell_h, self.ascent) = (m.w, m.h, m.ascent);
            self.measured = m;
            // The cached letters have the old size.
            self.glyphs.clear();
        }
    }

    /// The cell `height` times the font's line tall (`line_height` in the
    /// config), kept between 0.8 and 2. Like a size change: call
    /// `App::on_resize` after this.
    pub fn set_line_height(&mut self, height: f32) {
        self.line_height = height.clamp(0.8, 2.0);
        self.set_font_size(self.font_size);
    }

    /// A new display scale (the window moved to another display, or Windows'
    /// setting changed). Like a size change: call `App::on_resize` after this.
    pub fn set_text_scale(&mut self, scale: f32) {
        self.text_scale = scale;
        self.set_font_size(self.font_size);
    }

    /// New colors. The cells keep which color they want, so the next frame
    /// shows everything in the new theme, even text that is already there.
    /// How much room the project `name` needs before the tabs, at `scale` (see
    /// `Layout::set_lead`). No project: none.
    pub fn project_lead(&mut self, name: Option<&str>, scale: f32) -> usize {
        let Some(name) = name else {
            return 0;
        };
        if scale != self.ui.scale {
            let _ = self.ui.set_scale(&self.ui_font, scale);
        }
        let name_w = project_name_width(&project_name_shown(name), &self.ui);
        self.ui.px(PROJECT_NAME_LEFT) + name_w + self.ui.px(PROJECT_NAME_GAP)
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = self.glass.theme(&theme);
        self.solid_theme = solid(&self.glass, &theme);
        self.plain_theme = theme;
    }

    /// The project's name in pixels or not. Its width changes: call
    /// `App::on_resize` after this, for the tabs to move.
    pub fn set_pixel_name(&mut self, on: bool) {
        self.ui.pixel_name = on;
    }

    /// Kel's own cursor, for where the program hasn't asked for one.
    pub fn set_cursor(&mut self, shape: CursorShape) {
        self.cursor_shape = shape;
    }

    pub fn set_glass(&mut self, glass: Glass) {
        self.theme = glass.theme(&self.plain_theme);
        self.solid_theme = solid(&glass, &self.plain_theme);
        self.glass = glass;
    }

    /// Switches to another font family. Like a size change, the cell is measured
    /// again: call `App::on_resize` after this.
    pub fn set_fonts(&mut self, fonts: Fonts) {
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
        // The UI follows the layout's scale, which `App::on_resize` sets.
        if layout.scale != self.ui.scale {
            let _ = self.ui.set_scale(&self.ui_font, layout.scale);
        }
        let mut buffer = self.surface.buffer_mut().unwrap();
        let theme = match chrome.solid {
            true => &self.solid_theme,
            false => &self.theme,
        };
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
        let font = &self.ui_font; // all of the UI below: the built-in font
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
        draw_bar_buttons(&mut buffer, width, theme, layout, chrome);
        // A question waits for an answer now, so it comes before a problem. With
        // the sidebar open, it's at the sidebar's bottom instead: once is enough.
        let question = chrome.question.filter(|_| chrome.sidebar.is_none());
        // A link's address: its start (where it goes) and its end (which page,
        // which file), the middle cut if it's longer than half the bar.
        let half = (layout.tab_bar.w / 2) / self.ui.advance.max(1);
        let url = chrome.link_url.map(|url| cut_middle(url, half));
        let message = match (question, chrome.status) {
            (Some(question), _) => Some((question, theme.palette[11])),
            (None, Some(status)) => Some((status, theme.failed)),
            (None, None) => url
                .as_deref()
                .map(|url| (url, blend(theme.frame, theme.foreground, 150))),
        };
        if let Some((text, color)) = message {
            draw_status(
                &mut buffer,
                width,
                theme,
                layout,
                (text, color),
                font,
                &mut self.ui,
            );
        }

        let (cursor_row, cursor_col) = grid.cursor();
        // On a wide letter, the cursor covers both its cells.
        let cursor_wide = grid.cell(cursor_row, cursor_col).width == Width::Wide;
        let (cell_w, cell_h) = (self.cell_w, self.cell_h);
        // The program's cursor, or Kel's. With the keys elsewhere: a hollow box,
        // whatever the shape, shown all the time (no blinking where you don't type).
        let shape = grid.cursor_shape.unwrap_or(self.cursor_shape);
        let keys_here = chrome.keys_here;
        let cursor_shown = cursor_on || !keys_here;
        // Lines around the text (underline, cursor) grow with the text.
        let text_scale = self.text_scale;
        let text_px = |n: usize| scaled(n, text_scale);

        // The search's matches in view, by lasting row (see `Grid::dropped`).
        let top = grid.first_visible_row();
        let marks = chrome
            .search
            .map(|s| s.marks(top, top + grid.rows()))
            .unwrap_or_default();

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
                        && cursor_shown
                        && grid.scroll_offset() == 0
                        && row == cursor_row
                        && (col == cursor_col || cursor_wide && col == cursor_col + 1);
                    // A block cursor is drawn by swapping colors, so the letter stays visible.
                    let block = is_cursor && keys_here && shape == CursorShape::Block;

                    // Look the colors up in the theme. "Default" means a different
                    // color for text and background, so this comes before the swap.
                    let fg = theme.rgb(cell.fg, theme.foreground);
                    let bg = theme.rgb(cell.bg, theme.background);
                    let (fg, bg) = if cell.inverse { (bg, fg) } else { (fg, bg) };
                    // The chosen match: dark letters on bright yellow; the others,
                    // a yellow tint over their background. Both from the theme.
                    let (fg, bg) = match search::mark_at(&marks, top + row, col) {
                        Some(true) => (theme.background, theme.palette[11]),
                        Some(false) => (fg, blend(bg, theme.palette[3], MATCH_TINT)),
                        None => (fg, bg),
                    };
                    let bg = if selected { theme.selection } else { bg };
                    let (fg, bg) = if block { (bg, theme.cursor) } else { (fg, bg) };

                    if backgrounds {
                        fill_rect(&mut buffer, width, x, y, cell_w, cell_h, bg);
                        continue;
                    }

                    if cell.ch != ' ' && !invisible(cell.ch) {
                        let style = cell.bold as usize + 2 * cell.italic as usize;
                        let size = self.font_size * self.text_scale;
                        let glyph = glyph(
                            &mut self.glyphs,
                            &self.fonts[style],
                            &mut self.fallbacks,
                            (cell.ch, style),
                            size,
                            cell_w,
                        );
                        // A wide letter in the middle of its two cells.
                        let x = match cell.width {
                            Width::Wide => {
                                let room = 2.0 * cell_w as f32 - glyph.0.advance_width;
                                x + (room / 2.0).max(0.0) as usize
                            }
                            _ => x,
                        };
                        if fills_cell(cell.ch) {
                            let at = (x, y, cell_h);
                            draw_stretched(&mut buffer, width, glyph, self.measured, fg, at);
                        } else {
                            draw_glyph(&mut buffer, width, glyph, self.ascent, fg, x, y);
                        }
                    }

                    // The link under the mouse: a plain line under it, in the
                    // text's color, over whatever underline the cell had.
                    let here = (top + row, col);
                    let on_link = chrome
                        .link
                        .is_some_and(|l| l.start <= here && here <= l.end);
                    let underline = if on_link {
                        Underline::Single
                    } else {
                        cell.underline
                    };
                    if underline != Underline::None {
                        let color = match on_link {
                            true => fg,
                            false => theme.rgb(cell.underline_color, fg),
                        };
                        draw_underline(
                            &mut buffer,
                            width,
                            underline,
                            (x, y, cell_w, cell_h),
                            self.ascent as usize,
                            text_scale,
                            color,
                        );
                    }

                    if is_cursor && !keys_here {
                        // Once, around both cells of a wide letter.
                        if col == cursor_col {
                            let w = if cursor_wide { 2 * cell_w } else { cell_w };
                            let line = text_px(1).max(1);
                            let r = Rect { x, y, w, h: cell_h };
                            outline(&mut buffer, width, r, line, theme.cursor);
                        }
                        continue;
                    }
                    if is_cursor {
                        let (w, h) = match shape {
                            CursorShape::Underline => (cell_w, text_px(CURSOR_HEIGHT)),
                            CursorShape::Bar if col == cursor_col => (text_px(2), cell_h),
                            CursorShape::Bar => continue, // not again in a wide letter's 2nd cell
                            CursorShape::Block => continue, // drawn: the swapped colors above
                        };
                        fill_rect(&mut buffer, width, x, y + cell_h - h, w, h, theme.cursor);
                    }
                }
            }
        }

        // The bell: the panel a bit lighter, for a moment.
        if chrome.flash {
            for y in inside.y..inside.y + inside.h {
                let row = &mut buffer[y * width + inside.x..][..inside.w];
                for px in row {
                    *px = blend(*px, theme.foreground, FLASH);
                }
            }
        }

        // On top of the terminal, whatever the terminal shows. A logo's cursor
        // blinks where the keys go; one under something else stands, on.
        let over_start = chrome.sidebar.is_some()
            || chrome.palette.is_some()
            || chrome.search.is_some()
            || chrome.card.is_some()
            || chrome.help.is_some();
        let over_sidebar =
            chrome.palette.is_some() || chrome.card.is_some() || chrome.help.is_some();
        if let Some(start) = chrome.start {
            let on = cursor_on || over_start;
            draw_start(
                &mut buffer,
                width,
                theme,
                layout,
                start,
                on,
                font,
                &mut self.ui,
            );
        }
        if let Some(search) = chrome.search {
            draw_search(
                &mut buffer,
                width,
                theme,
                layout,
                search,
                font,
                &mut self.ui,
            );
        }
        if let Some(sidebar) = chrome.sidebar {
            draw_sidebar(
                &mut buffer,
                width,
                theme,
                layout,
                chrome,
                sidebar,
                cursor_on || over_sidebar,
                font,
                &mut self.ui,
            );
        }
        if let Some((card, rect)) = chrome.card {
            draw_card(
                &mut buffer,
                width,
                theme,
                layout,
                card,
                rect,
                font,
                &mut self.ui,
            );
        }
        // Last: the palette is on top of everything.
        if let Some(palette) = chrome.palette {
            // The project's color (or the theme's accent): its chosen line's.
            let color = chrome.project.and_then(|(_, color)| color).or(theme.accent);
            draw_palette(
                &mut buffer,
                width,
                theme,
                layout,
                (palette, color),
                font,
                &mut self.ui,
            );
        }
        // Except the help, which closes the others when it opens.
        if let Some(help) = chrome.help {
            draw_help(&mut buffer, width, theme, layout, help, font, &mut self.ui);
        }

        if self.glass.on() {
            self.glass.finish(&mut buffer, width);
        }
        buffer.present().unwrap();
    }
}

/// The tab bar. The tab on screen merges into the panel below it: the panel's
/// color inside, the panel's outline around, and no line between them. The other
/// tabs are only their names, dimmer, on the window's color. Each tab has an
/// icon before its name, and a dot at its right end when it has new output.
fn draw_tab_bar(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    chrome: &Chrome,
    font: &Font,
    ui: &mut Ui,
) {
    let Chrome { tabs, active, .. } = *chrome;
    // Halfway between the text color and the window's: muted, still readable.
    let dim = blend(theme.frame, theme.foreground, 128);
    let b = layout.border;
    // The window's accent: the project's own color (home: none, gray).
    let accent = theme.accent_for(chrome.project.and_then(|(_, color)| color));
    // The project's name, at the bar's start, where `Layout::set_lead` left room.
    if let Some((name, _)) = chrome.project {
        let bar = layout.tab_bar;
        let shown = project_name_shown(name);
        let at = (
            layout.sidebar_button.x + layout.sidebar_button.w + ui.px(PROJECT_NAME_LEFT),
            bar.y + bar.h.saturating_sub(ui.line_h) / 2,
        );
        match project_name_pixels(&shown, ui) {
            // In pixels, like the logo: its 7 rows down to the line in the bar's
            // middle, g's tail below.
            Some((lit, _)) => {
                let px = project_name_px(ui);
                let top = bar.y + bar.h.saturating_sub(logo::HEIGHT * px) / 2;
                for (x, y) in lit {
                    fill_rect(pixels, width, at.0 + x * px, top + y * px, px, px, accent);
                }
            }
            // A letter the pixel font hasn't got: all of it in the UI's font.
            None => {
                let max_w = shown.chars().count() * ui.advance;
                draw_ui_text(pixels, width, font, ui, &shown, at, max_w, accent);
            }
        }
    }
    for (i, (tab, label)) in layout.tabs(tabs.len()).iter().zip(tabs).enumerate() {
        let on_screen = i == active;
        let color = if on_screen {
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
        // The icon, in the middle from top to bottom. The tab on screen has it in
        // the window's color (the project's, gray in home); a muted tab, in the
        // muted gray of its name.
        let icon = ui.icons.get(label.icon);
        let (icon_w, icon_h) = (icon.w * ui.icon_scale, icon.h * ui.icon_scale);
        let padding = ui.px(TAB_TEXT_PADDING);
        let icon_x = tab.x + padding;
        let icon_y = tab.y + tab.h.saturating_sub(icon_h) / 2;
        let tint = if on_screen { accent } else { dim };
        draw_icon(pixels, width, icon, ui.icon_scale, tint, icon_x, icon_y);

        let gap = ui.px(ICON_GAP);
        let mut max_w = tab.w.saturating_sub(2 * padding + icon_w + gap);
        // Under the mouse: a × at the right end, in place of the mark.
        let hovered = match chrome.hovered {
            Some(TitleHit::Tab { index, close }) if index == i => Some(close),
            _ => None,
        };
        if let Some(on_close) = hovered {
            let r = layout.tab_close(*tab);
            let tab_bg = if on_screen {
                theme.background
            } else {
                theme.frame
            };
            if on_close {
                let lit = blend(tab_bg, theme.foreground, 40);
                fill_rect(pixels, width, r.x, r.y, r.w, r.h, lit);
            }
            let (n, t) = (ui.px(8), ui.px(1));
            let (x, y) = (r.x + (r.w - n) / 2, r.y + (r.h - n) / 2);
            draw_x(pixels, width, (x, y), n, t, color);
            max_w = max_w.saturating_sub(r.w + gap);
        }
        // The mark at the right end, one at a time: a running command's ring
        // says the most, then how one ended, then plain new output.
        let mark = match (label.busy, label.done) {
            _ if hovered.is_some() => None,
            (true, _) => Some((theme.foreground, RING_WIDTH)),
            // Kel's soft green and red, not the palette's loud ones.
            (false, Some(true)) => Some((theme.done, 0.0)),
            (false, Some(false)) => Some((theme.failed, 0.0)),
            (false, None) if label.activity => Some((theme.foreground, 0.0)),
            _ => None,
        };
        if let Some((mark_color, ring)) = mark {
            let radius = DOT_RADIUS * ui.scale;
            // `saturating_sub`: a tab can be narrower than its padding (in a
            // tiny window, or with a hundred tabs).
            let cx = (tab.x + tab.w).saturating_sub(padding) as f32 - radius;
            let cy = tab.y as f32 + tab.h as f32 / 2.0;
            // A ring is the dot with a hole: the line is `ring` wide.
            let hole = if ring > 0.0 {
                radius - ring * ui.scale
            } else {
                0.0
            };
            fill_circle(pixels, width, cx, cy, radius, hole, mark_color);
            // A long name stops before the mark, with a mark's width between them.
            max_w = max_w.saturating_sub((4.0 * radius) as usize);
        }
        // The name, in the middle from top to bottom, cut off where the tab ends.
        let x = icon_x + icon_w + gap;
        let y = tab.y + tab.h.saturating_sub(ui.line_h) / 2;
        draw_ui_text(pixels, width, font, ui, label.title, (x, y), max_w, color);
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
        let lit_now = chrome.hovered == Some(TitleHit::Window(button));
        let (bg, fg) = match (button, lit_now) {
            (WindowButton::Close, true) => (CLOSE_HOVER, 0xffffff),
            (_, true) => (lit, theme.foreground),
            (_, false) => (theme.frame, dim),
        };
        fill_rect(pixels, width, rect.x, rect.y, rect.w, rect.h, bg);

        // The icon: 10x10 pixels at 100%, in the middle of the button, with
        // lines `t` thick (1 pixel at 100%, 2 at 200%).
        let s = |n: usize| scaled(n, layout.scale);
        let (n, t) = (s(10), s(1));
        let (x, y) = (
            (rect.x + rect.w / 2).saturating_sub(n / 2),
            (rect.y + rect.h / 2).saturating_sub(n / 2),
        );
        match button {
            WindowButton::Minimize => fill_rect(pixels, width, x, y + n / 2, n, t, fg),
            WindowButton::Maximize if chrome.maximized => {
                // "Restore": a square, and the corner of another one behind it.
                let (offset, side) = (s(2), s(8));
                let front = Rect {
                    x,
                    y: y + offset,
                    w: side,
                    h: side,
                };
                outline(pixels, width, front, t, fg);
                fill_rect(pixels, width, x + offset, y, side, t, fg);
                fill_rect(pixels, width, x + n - t, y, t, side, fg);
            }
            WindowButton::Maximize => outline(pixels, width, Rect { x, y, w: n, h: n }, t, fg),
            WindowButton::Close => draw_x(pixels, width, (x, y), n, t, fg),
        }
    }
}

/// `+` (a new tab) and `⌄` (the command palette), right after the last tab: dim,
/// with a lighter background under the mouse, like the window buttons.
fn draw_bar_buttons(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    chrome: &Chrome,
) {
    let dim = blend(theme.frame, theme.foreground, 128);
    let lit = blend(theme.frame, theme.foreground, 40);
    let (new_tab, palette) = layout.bar_buttons(chrome.tabs.len());
    let s = |n: usize| scaled(n, layout.scale);
    let buttons = [
        (layout.sidebar_button, TitleHit::Sidebar),
        (new_tab, TitleHit::NewTab),
        (palette, TitleHit::Palette),
    ];
    for (r, hit) in buttons {
        // A square in the middle of the button's place, 24 x 24 at 100%.
        let side = s(24).min(r.w).min(r.h);
        let (bx, by) = (r.x + (r.w - side) / 2, r.y + (r.h - side) / 2);
        // The sidebar's button stays lit while its sidebar is open.
        let open = hit == TitleHit::Sidebar && chrome.sidebar.is_some();
        let fg = if chrome.hovered == Some(hit) || open {
            fill_rect(pixels, width, bx, by, side, side, lit);
            theme.foreground
        } else {
            dim
        };
        // The icon: 10 x 10 at 100%, lines 1 pixel thick.
        let (n, t) = (s(10), s(1));
        let (x, y) = (bx + (side - n) / 2, by + (side - n) / 2);
        match hit {
            // A panel with a column on its left: what it opens. 12 x 10 at 100%,
            // the column 4 wide.
            TitleHit::Sidebar => {
                let (w, x) = (s(12), bx + (side - s(12)) / 2);
                fill_rect(pixels, width, x, y, w, t, fg);
                fill_rect(pixels, width, x, y + n - t, w, t, fg);
                fill_rect(pixels, width, x, y, t, n, fg);
                fill_rect(pixels, width, x + w - t, y, t, n, fg);
                fill_rect(pixels, width, x + s(4), y, t, n, fg);
            }
            // Two lines through the middle.
            TitleHit::NewTab => {
                fill_rect(pixels, width, x, y + (n - t) / 2, n, t, fg);
                fill_rect(pixels, width, x + (n - t) / 2, y, t, n, fg);
            }
            // A V, half as tall as wide, a bit below the middle: a t x t square
            // per row from each side, meeting in the middle.
            _ => {
                let y = y + n / 4;
                for i in 0..=(n - t) / 2 {
                    fill_rect(pixels, width, x + i, y + i, t, t, fg);
                    fill_rect(pixels, width, x + n - t - i, y + i, t, t, fg);
                }
            }
        }
    }
}

/// An X, `n` pixels square, from (x, y): two diagonals, a t x t square per row.
fn draw_x(
    pixels: &mut [u32],
    width: usize,
    (x, y): (usize, usize),
    n: usize,
    t: usize,
    color: u32,
) {
    for i in 0..=(n - t) {
        fill_rect(pixels, width, x + i, y + i, t, t, color);
        fill_rect(pixels, width, x + n - t - i, y + i, t, t, color);
    }
}

/// A message in its color at the right end of the tab bar, just before the window
/// buttons. It goes over the tabs if they reach that far: it should be seen.
fn draw_status(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    (status, color): (&str, u32),
    font: &Font,
    ui: &mut Ui,
) {
    let bar = layout.tab_bar;
    // At most the right half of the bar; a longer message is cut.
    let max_chars = (bar.w / 2) / ui.advance.max(1);
    let shown: String = status.chars().take(max_chars).collect();
    let text_w = shown.chars().count() * ui.advance;
    let end = bar.x + bar.w;
    let x = end.saturating_sub(text_w);
    // The window's color behind it, so tabs under it don't show through.
    let back = x.saturating_sub(ui.px(TAB_TEXT_PADDING));
    fill_rect(pixels, width, back, bar.y, end - back, bar.h, theme.frame);
    let y = bar.y + bar.h.saturating_sub(ui.line_h) / 2;
    draw_ui_text(pixels, width, font, ui, &shown, (x, y), text_w, color);
}

/// The command palette, at the top of the terminal panel, in the middle:
///
/// ```text
/// ┌──────────────────────────────┐
/// │ > zo|                        │  what's typed, and a cursor
/// ├──────────────────────────────┤
/// │ Zoom in              Ctrl+=  │  the chosen line, in the selection color
/// │ Zoom out             Ctrl+-  │
/// └──────────────────────────────┘
/// ```
fn draw_palette(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    (palette, color): (&Palette, Option<u32>),
    font: &Font,
    ui: &mut Ui,
) {
    let area = layout.inside;
    let pad = ui.px(PALETTE_PADDING);
    let b = layout.border.max(1);
    let line_h = ui.line_h + 2 * pad;
    // As wide as 64 letters, or the panel minus some room at the sides.
    let box_w = (PALETTE_CHARS * ui.advance + 2 * pad).min(area.w.saturating_sub(4 * pad));
    if box_w < 10 * ui.advance {
        return; // a window too narrow for it
    }
    let x = area.x + (area.w - box_w) / 2;
    let y = area.y + 2 * pad;

    // The lines on screen: the chosen one is always among them. With 10 lines and
    // line 12 chosen, lines 3 to 12.
    let shown: Vec<_> = palette.shown().collect();
    let first = (palette.selected() + 1).saturating_sub(PALETTE_LINES);
    let lines = shown.len().clamp(1, PALETTE_LINES); // "nothing found" takes a line too
    let box_h = line_h + b + lines * line_h + 2 * b;

    let dim = blend(theme.frame, theme.foreground, 128);
    outline(
        pixels,
        width,
        Rect {
            x,
            y,
            w: box_w,
            h: box_h,
        },
        b,
        theme.border,
    );
    fill_rect(
        pixels,
        width,
        x + b,
        y + b,
        box_w - 2 * b,
        box_h - 2 * b,
        theme.frame,
    );
    let text_w = box_w - 2 * (b + pad);
    let text_x = x + b + pad;

    // The search box: "> ", what's typed, and a bar for the cursor after it.
    let query = format!("> {}", palette.query());
    let query_y = y + b + pad;
    let typed = draw_ui_text(
        pixels,
        width,
        font,
        ui,
        &query,
        (text_x, query_y),
        text_w,
        theme.foreground,
    );
    fill_rect(
        pixels,
        width,
        text_x + typed,
        query_y,
        ui.px(2),
        ui.line_h,
        theme.cursor,
    );
    let sep_y = y + b + line_h;
    fill_rect(pixels, width, x, sep_y, box_w, b, theme.border);

    let list_y = sep_y + b;
    if shown.is_empty() {
        let at = (text_x, list_y + pad);
        draw_ui_text(pixels, width, font, ui, "Nothing found", at, text_w, dim);
    }
    for (row, (i, item)) in shown
        .iter()
        .enumerate()
        .skip(first)
        .take(PALETTE_LINES)
        .enumerate()
    {
        let line_y = list_y + row * line_h;
        // The chosen line: in a project, a tint of its color (60 of 255: the
        // text and the hint still read on it); at home (no color), gray.
        if i == palette.selected() {
            let fill = match color {
                Some(color) => blend(theme.frame, color, 60),
                None => theme.selection,
            };
            fill_rect(pixels, width, x + b, line_y, box_w - 2 * b, line_h, fill);
        }
        // The hint at the right end, then the label in the room left of it.
        // At most half the line, so a long folder leaves the label room.
        let hint_w = (item.hint.chars().count() * ui.advance).min(text_w / 2);
        let hint_x = (text_x + text_w).saturating_sub(hint_w);
        draw_ui_hint(
            pixels,
            width,
            ui,
            &item.hint,
            (hint_x, line_y + pad),
            hint_w,
            dim,
        );
        let label_w = text_w.saturating_sub(hint_w + 2 * ui.advance);
        let at = (text_x, line_y + pad);
        draw_ui_text(
            pixels,
            width,
            font,
            ui,
            &item.label,
            at,
            label_w,
            theme.foreground,
        );
    }
}

/// The sidebar (see `sidebar`): the window dimmed, the panel over its left, and
/// in it the logo, the projects (a dot of each one's color), "Save", the keys.
#[allow(clippy::too_many_arguments)]
fn draw_sidebar(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    chrome: &Chrome,
    sidebar: &Sidebar,
    cursor_on: bool,
    font: &Font,
    ui: &mut Ui,
) {
    for pixel in pixels.iter_mut() {
        *pixel = blend(*pixel, 0x000000, SIDEBAR_DIM);
    }
    let panel = layout.sidebar();
    let b = layout.border.max(1);
    let back = blend(theme.frame, theme.foreground, 8);
    outline(pixels, width, panel, b, theme.border);
    let inside = panel.shrink(b);
    fill_rect(pixels, width, inside.x, inside.y, inside.w, inside.h, back);
    let dim = blend(back, theme.foreground, 128);
    let pad = ui.px(18);
    let text_w = panel.w.saturating_sub(2 * pad);

    // The logo, small: its cursor the project's color (home: none, the text's),
    // blinking while the keys are the sidebar's.
    let at = (panel.x + pad, panel.y + ui.px(22));
    let cursor = theme.accent_for(chrome.project.and_then(|(_, color)| color));
    let cursor = cursor_on.then_some(cursor);
    draw_logo(pixels, width, at, ui.px(3), theme.foreground, cursor);
    let at = (panel.x + pad, sidebar::title_y(panel, layout.scale));
    draw_ui_text(pixels, width, font, ui, "PROJECTS", at, text_w, dim);

    let projects = sidebar.projects();
    for row in 0..sidebar.rows() {
        let Some(r) = sidebar.row_rect(panel, layout.scale, row) else {
            continue; // scrolled out of view, or no room
        };
        let chosen = row == sidebar.selected();
        if chosen {
            let lit = blend(back, theme.foreground, 22);
            fill_rect(pixels, width, r.x, r.y, r.w, r.h, lit);
        }
        let text_y = r.y + r.h.saturating_sub(ui.line_h) / 2;
        let look = match sidebar.row(row) {
            Row::Save => {
                // After a line across.
                let line_y = r.y - ui.px(8);
                let line = theme.border;
                fill_rect(pixels, width, panel.x + pad, line_y, text_w, 1, line);
                let color = if chosen { theme.foreground } else { dim };
                let label = "+ Save the tabs as a project";
                let at = (r.x + ui.px(10), text_y);
                draw_ui_text(pixels, width, font, ui, label, at, r.w, color);
                continue;
            }
            // A ring, not a dot: no project, no color of its own.
            Row::Home { here, live } => ProjectRow {
                name: "home",
                folder: "",
                color: None,
                ring: true,
                here,
                live,
            },
            Row::Project(entry) => ProjectRow::of(entry),
        };
        draw_project_row(pixels, width, theme, font, ui, back, r, &look);
    }
    // More projects than fit: a thin bar at the list's right, as long as the
    // part shown, where it is in the whole list.
    let (above, below) = sidebar.hidden();
    if above + below > 0 {
        let list = sidebar.list_rect(panel, layout.scale);
        let n = projects.len();
        let top = list.y + list.h * above / n;
        let h = (list.h * (n - above - below) / n).max(ui.px(8));
        let x = panel.x + panel.w - b - ui.px(5);
        fill_rect(pixels, width, x, top, ui.px(2), h, dim);
    }
    if projects.is_empty() {
        // Where the first project would be.
        let r = sidebar::empty_rect(panel, layout.scale);
        let at = (r.x + ui.px(10), r.y + r.h.saturating_sub(ui.line_h) / 2);
        draw_ui_text(pixels, width, font, ui, "no projects yet", at, r.w, dim);
    }

    // At the bottom: the keys, or a question waiting for a second press (Del
    // on a project where a command runs), in two lines if it's long.
    let footer = sidebar::footer_y(panel, layout.scale);
    // The keys: in two lines, all of them don't fit in one.
    let fits = text_w / ui.advance.max(1);
    let (lines, color): (Vec<&str>, u32) = match chrome.question {
        Some(text) => match text.split_once(": ") {
            Some((first, rest)) if text.chars().count() > fits => {
                (vec![first, rest], theme.foreground)
            }
            _ => (vec![text], theme.foreground),
        },
        None => (vec!["F2 edit   Del close", "Shift+Del delete   Esc"], dim),
    };
    let top = footer + ui.px(24) - lines.len() * ui.line_h / 2;
    for (i, line) in lines.into_iter().enumerate() {
        let at = (panel.x + pad, top + i * ui.line_h);
        if color == dim {
            draw_ui_hint(pixels, width, ui, line, at, text_w, color);
        } else {
            draw_ui_text(pixels, width, font, ui, line, at, text_w, color);
        }
    }
}

/// The card (see `card`) at `rect`: each field's name and what's typed in it,
/// the cursor in the one it's in, and under them what that field does.
#[allow(clippy::too_many_arguments)]
fn draw_card(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    card: &Card,
    rect: Rect,
    font: &Font,
    ui: &mut Ui,
) {
    let b = layout.border.max(1);
    outline(pixels, width, rect, b, theme.border);
    let inside = rect.shrink(b);
    let back = theme.frame;
    fill_rect(pixels, width, inside.x, inside.y, inside.w, inside.h, back);
    let dim = blend(back, theme.foreground, 128);
    let pad = ui.px(8);
    // "Command" and a space.
    let label_w = 8 * ui.advance;
    for (i, field) in card.fields().iter().enumerate() {
        let r = card::field_rect(rect, layout.scale, i);
        let focused = i == card.focus();
        let text_y = r.y + r.h.saturating_sub(ui.line_h) / 2;
        draw_ui_text(
            pixels,
            width,
            font,
            ui,
            field.label,
            (r.x + pad, text_y),
            label_w,
            dim,
        );
        // A color's field: its swatches first, the chosen one ringed. The box
        // for the `#hex` after them.
        let mut bx = r.x + pad + label_w;
        if !field.swatches.is_empty() {
            let cy = r.y as f32 + r.h as f32 / 2.0;
            let radius = 6.0 * ui.scale;
            let chosen = field.swatch();
            for (n, swatch) in field.swatches.iter().enumerate() {
                let cx = (bx + ui.px(10) + n * ui.px(22)) as f32;
                match swatch {
                    Some(color) => fill_circle(pixels, width, cx, cy, radius, 0.0, *color),
                    // None: the window's color without one (gray), as a ring.
                    None => {
                        let hole = radius - 1.5 * ui.scale;
                        let color = theme.accent_for(None);
                        fill_circle(pixels, width, cx, cy, radius, hole, color);
                    }
                }
                if chosen == Some(n) {
                    let outer = radius + 3.0 * ui.scale;
                    let hole = outer - RING_WIDTH * ui.scale;
                    fill_circle(pixels, width, cx, cy, outer, hole, theme.foreground);
                }
            }
            bx += field.swatches.len() * ui.px(22) + ui.px(8);
        }
        // The box it's typed in: lighter where the cursor is.
        let (by, bh) = (r.y + ui.px(3), r.h.saturating_sub(ui.px(6)));
        let bw = (r.x + r.w).saturating_sub(bx);
        let lit = if focused { 20 } else { 8 };
        fill_rect(
            pixels,
            width,
            bx,
            by,
            bw,
            bh,
            blend(back, theme.foreground, lit),
        );
        let mut tx = bx + ui.px(6);
        // A color's `#hex`: a dot of it before, as it's typed (none: not one yet).
        if !field.swatches.is_empty() {
            let cy = r.y as f32 + r.h as f32 / 2.0;
            let color = field.color().map(|c| theme.accent_for(c));
            if let Some(color) = color {
                let dot = 5.0 * ui.scale;
                fill_circle(pixels, width, (tx + ui.px(5)) as f32, cy, dot, 0.0, color);
            }
            tx += ui.px(16);
        }
        let chars = (bx + bw).saturating_sub(tx + ui.px(6)) / ui.advance.max(1);
        let (shown, cursor) = field.text.visible(chars);
        let text_w = chars * ui.advance;
        if field.text.text().is_empty() {
            // Empty: what that means, dim.
            draw_ui_text(
                pixels,
                width,
                font,
                ui,
                field.empty,
                (tx, text_y),
                text_w,
                dim,
            );
        } else {
            let fg = theme.foreground;
            draw_ui_text(pixels, width, font, ui, &shown, (tx, text_y), text_w, fg);
        }
        if focused {
            let cx = tx + cursor * ui.advance;
            fill_rect(pixels, width, cx, text_y, ui.px(2), ui.line_h, theme.cursor);
        }
    }
    // Under the fields: what the one with the cursor does, and the keys. Or,
    // in red, why what's typed can't be kept.
    let n = card.fields().len();
    let note = card.fields()[card.focus()].note;
    let y = card::hint_y(rect, layout.scale, n) + ui.px(6);
    let w = rect.w.saturating_sub(2 * pad);
    let at = (rect.x + 2 * pad, y);
    match &card.error {
        Some(error) => {
            draw_ui_text(pixels, width, font, ui, error, at, w, theme.failed);
        }
        None => {
            let hint = format!("{note}   Enter   Esc");
            draw_ui_hint(pixels, width, ui, &hint, at, w, dim);
        }
    };
}

/// A row of projects, in the sidebar or on the start screen.
struct ProjectRow<'a> {
    name: &'a str,
    /// Where it is: shown when it isn't open.
    folder: &'a str,
    /// Its dot's color; `None`: the text's.
    color: Option<u32>,
    /// A ring, not a dot: home, no project of its own.
    ring: bool,
    /// It's on screen.
    here: bool,
    live: Live,
}

impl<'a> ProjectRow<'a> {
    fn of(entry: &'a Entry) -> ProjectRow<'a> {
        ProjectRow {
            name: &entry.name,
            folder: &entry.hint,
            color: entry.color,
            ring: false,
            here: entry.current,
            live: entry.live,
        }
    }
}

/// A project in the row `r`, on the background `back`: a dot of its color (a
/// ring around it if a command runs there), its name, and at the right where it
/// is, or what it's doing. Bright if it's open in the window, faded if not.
#[allow(clippy::too_many_arguments)]
fn draw_project_row(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    font: &Font,
    ui: &mut Ui,
    back: u32,
    r: Rect,
    row: &ProjectRow,
) {
    let dim = blend(back, theme.foreground, 128);
    let text_y = r.y + r.h.saturating_sub(ui.line_h) / 2;
    let radius = 4.0 * ui.scale;
    let (cx, cy) = ((r.x + ui.px(14)) as f32, r.y as f32 + r.h as f32 / 2.0);
    let live = row.live;
    // Open in the window: bright. Not open: faded, its name too.
    let awake = row.here || live.open;
    let color = theme.accent_for(row.color);
    let color = if awake {
        color
    } else {
        blend(back, color, 100)
    };
    let hole = if row.ring {
        radius - 1.5 * ui.scale
    } else {
        0.0
    };
    fill_circle(pixels, width, cx, cy, radius, hole, color);
    // A command runs there: a ring around it, as around a tab's mark.
    if live.busy {
        let outer = radius + 3.0 * ui.scale;
        let hole = outer - RING_WIDTH * ui.scale;
        fill_circle(pixels, width, cx, cy, outer, hole, color);
    }
    // At the right: on screen, what it's doing, or where it is.
    let hint = match () {
        _ if row.here => "here",
        _ if live.busy => "running",
        _ if live.open => "open",
        _ => row.folder,
    };
    let name_x = r.x + ui.px(28);
    let hint = keep_end(hint, (r.w / 2) / ui.advance.max(1));
    let hint_w = hint.chars().count() * ui.advance;
    let hint_x = r.x + r.w - ui.px(10) - hint_w;
    // New output not seen yet: a dot before the hint, as on a tab.
    let mark_w = if live.activity && !row.here {
        let dot = DOT_RADIUS * ui.scale;
        let mx = hint_x as f32 - ui.px(8) as f32 - dot;
        fill_circle(pixels, width, mx, cy, dot, 0.0, theme.foreground);
        ui.px(16)
    } else {
        0
    };
    let name_w = hint_x.saturating_sub(name_x + mark_w + ui.px(8));
    let fg = if awake {
        theme.foreground
    } else {
        blend(back, theme.foreground, 170)
    };
    let at = (name_x, text_y);
    draw_ui_text(pixels, width, font, ui, row.name, at, name_w, fg);
    let at = (hint_x, text_y);
    draw_ui_text(pixels, width, font, ui, &hint, at, hint_w, dim);
}

/// The start screen (see `start`) over the terminal: Kel's name, the projects
/// used last, the keys.
#[allow(clippy::too_many_arguments)]
fn draw_start(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    start: &Start,
    cursor_on: bool,
    font: &Font,
    ui: &mut Ui,
) {
    let area = layout.inside;
    let back = theme.background;
    fill_rect(pixels, width, area.x, area.y, area.w, area.h, back);
    let dim = blend(back, theme.foreground, 128);
    let projects = start.projects();
    let col = start::column(layout.text, layout.scale, projects.len());
    let left = col.x + ui.px(10);

    // Behind it all: the backdrop, clear around the logo and the list (and the
    // hint's two lines).
    let bottom = start::hint_y(col, layout.scale, projects.len()) + 2 * ui.line_h;
    let clear = Rect {
        h: bottom.saturating_sub(col.y),
        ..col
    };
    if let Some(backdrop) = start.backdrop() {
        draw_backdrop(pixels, width, theme, area, clear, backdrop, font, ui);
    }

    // The logo, big, its cursor blinking with the terminal's: type, it says.
    // Home: no color, the text's.
    let cursor = cursor_on.then_some(theme.accent_for(None));
    draw_logo(
        pixels,
        width,
        (left, col.y),
        ui.px(6),
        theme.foreground,
        cursor,
    );
    let at = (left, start::title_y(col, layout.scale));
    draw_ui_text(pixels, width, font, ui, "RECENT PROJECTS", at, col.w, dim);

    for (row, entry) in projects.iter().enumerate() {
        let r = start::row_rect(col, layout.scale, row);
        if !start::fits(layout.text, r) {
            break;
        }
        if start.lit(row) {
            let lit = blend(back, theme.foreground, 22);
            fill_rect(pixels, width, r.x, r.y, r.w, r.h, lit);
        }
        draw_project_row(
            pixels,
            width,
            theme,
            font,
            ui,
            back,
            r,
            &ProjectRow::of(entry),
        );
    }
    if projects.is_empty() {
        let r = start::row_rect(col, layout.scale, 0);
        let at = (left, r.y + r.h.saturating_sub(ui.line_h) / 2);
        let text = "none yet: Ctrl+Shift+P, \"Save the tabs as project\"";
        draw_ui_hint(pixels, width, ui, text, at, col.w, dim);
    }
    let hint_y = start::hint_y(col, layout.scale, projects.len());
    if hint_y + ui.line_h <= layout.text.y + layout.text.h {
        let text = "Type to start   ↑↓ Enter opens a project   Esc";
        draw_ui_hint(pixels, width, ui, text, (left, hint_y), col.w, dim);
    }
    // Where the rest is: all the keys, the settings.
    let help_y = hint_y + ui.line_h;
    if help_y + ui.line_h <= layout.text.y + layout.text.h {
        let text = "Ctrl+Shift+/ all the keys and settings";
        draw_ui_hint(pixels, width, ui, text, (left, help_y), col.w, dim);
    }
}

/// A column of the help on screen: its lines, where it starts and how wide it
/// is (pixels), how wide its keys and its values are (letters).
struct HelpColumn<'a> {
    lines: &'a [Line],
    x: usize,
    w: usize,
    key_w: usize,
    value_w: usize,
}

/// The help (see `help`): a box over the panel, the keys and the settings side
/// by side if both fit, else one after the other; scrolled to `help.scroll()`.
/// A setting's color: a dot of it before its `#hex`.
fn draw_help(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    help: &Help,
    font: &Font,
    ui: &mut Ui,
) {
    let area = layout.inside;
    let margin = ui.px(16);
    let pad = ui.px(16);
    let b = layout.border.max(1);
    let rect = Rect {
        x: area.x + margin,
        y: area.y + margin,
        w: area.w.saturating_sub(2 * margin),
        h: area.h.saturating_sub(2 * margin),
    };
    let advance = ui.advance.max(1);
    let row_h = ui.line_h + ui.px(4);
    if rect.w < 2 * (b + pad) + 24 * advance || rect.h < 2 * (b + pad) + 4 * row_h {
        return; // a window too small for it
    }
    outline(pixels, width, rect, b, theme.border);
    let (inner_w, inner_h) = (rect.w - 2 * b, rect.h - 2 * b);
    fill_rect(
        pixels,
        width,
        rect.x + b,
        rect.y + b,
        inner_w,
        inner_h,
        theme.frame,
    );
    let text_x = rect.x + b + pad;
    let text_w = rect.w - 2 * (b + pad);
    let top = rect.y + b + pad;
    // Under the lines: the keys' hint, a line of its own.
    let hint_y = rect.y + rect.h - b - pad - ui.line_h;
    let shown = hint_y.saturating_sub(top + ui.px(8)) / row_h;

    // Each column's widths, in letters: its keys, its values, its notes.
    let widths = |lines: &[Line]| {
        let (mut key, mut value, mut note) = (0, 0, 0);
        for line in lines {
            if let Line::Row {
                key: k,
                value: v,
                note: n,
                color,
            } = line
            {
                key = key.max(k.chars().count() + 2);
                // A color's dot takes 2 letters' room before it.
                let dot = if color.is_some() { 2 } else { 0 };
                value = value.max(v.chars().count() + dot + 2);
                note = note.max(n.chars().count());
            }
        }
        (key, value, note)
    };
    let [keys, settings] = help.columns();
    let (kw, kv, kn) = widths(keys);
    let (sw, sv, sn) = widths(settings);
    let gap = ui.px(40);
    let need = |w: usize, v: usize, n: usize| (w + v + n) * advance;
    let two = need(kw, kv, kn) + gap + need(sw, sv, sn) <= text_w;
    let one: Vec<Line>;
    let columns: Vec<HelpColumn> = if two {
        let left_w = need(kw, kv, kn);
        vec![
            HelpColumn {
                lines: keys,
                x: text_x,
                w: left_w,
                key_w: kw,
                value_w: kv,
            },
            HelpColumn {
                lines: settings,
                x: text_x + left_w + gap,
                w: text_w - left_w - gap,
                key_w: sw,
                value_w: sv,
            },
        ]
    } else {
        one = keys
            .iter()
            .cloned()
            .chain([Line::Gap])
            .chain(settings.iter().cloned())
            .collect();
        vec![HelpColumn {
            lines: &one,
            x: text_x,
            w: text_w,
            key_w: kw.max(sw),
            value_w: kv.max(sv),
        }]
    };
    let lines = columns.iter().map(|c| c.lines.len()).max().unwrap_or(0);
    help.fit(lines, shown);
    let scroll = help.scroll().min(lines.saturating_sub(shown));

    let dim = blend(theme.frame, theme.foreground, 128);
    let soft = blend(theme.frame, theme.foreground, 200);
    for HelpColumn {
        lines,
        x,
        w,
        key_w,
        value_w,
    } in columns
    {
        let value_x = x + key_w * advance;
        let note_x = value_x + value_w * advance;
        for (i, line) in lines.iter().skip(scroll).take(shown).enumerate() {
            let y = top + i * row_h + (row_h - ui.line_h) / 2;
            match line {
                Line::Title(text) => {
                    draw_ui_text(pixels, width, font, ui, text, (x, y), w, dim);
                }
                Line::Row {
                    key,
                    value,
                    note,
                    color,
                } => {
                    draw_ui_text(pixels, width, font, ui, key, (x, y), w, theme.foreground);
                    let mut vx = value_x;
                    if let Some(color) = color {
                        let r = 5.0 * ui.scale;
                        let cy = y as f32 + ui.line_h as f32 / 2.0;
                        let cx = (vx + advance / 2) as f32 + r / 2.0;
                        fill_circle(pixels, width, cx, cy, r, 0.0, *color);
                        vx += 2 * advance;
                    }
                    let room = (x + w).saturating_sub(vx);
                    draw_ui_text(pixels, width, font, ui, value, (vx, y), room, soft);
                    let room = (x + w).saturating_sub(note_x);
                    draw_ui_text(pixels, width, font, ui, note, (note_x, y), room, dim);
                }
                Line::Gap => {}
            }
        }
    }
    // The keys, and whether there's more below or above.
    let more = match (scroll > 0, scroll + shown < lines) {
        (_, true) => "↓ more   ↑↓ PgUp PgDn scroll   Esc",
        (true, false) => "↑↓ PgUp PgDn scroll   Esc",
        (false, false) => "Esc",
    };
    draw_ui_hint(pixels, width, ui, more, (text_x, hint_y), text_w, dim);
}

/// The start screen's backdrop (see `backdrop`) over `area`, a cell a letter of
/// the UI's font. Its marks fade out near `clear`, where the logo and the list
/// are: gone inside, whole from `FADE` pixels away.
#[allow(clippy::too_many_arguments)]
fn draw_backdrop(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    area: Rect,
    clear: Rect,
    backdrop: &Backdrop,
    font: &Font,
    ui: &mut Ui,
) {
    const FADE: usize = 110;
    let fade = ui.px(FADE).max(1) as f32;
    let (cell_w, cell_h) = (ui.advance.max(1), ui.line_h.max(1));
    let pad = ui.px(8);
    let cols = area.w.saturating_sub(2 * pad) / cell_w;
    let rows = area.h.saturating_sub(2 * pad) / cell_h;
    for row in 0..rows {
        for col in 0..cols {
            let Some(mark) = backdrop.mark(col, row, rows) else {
                continue;
            };
            let (x, y) = (area.x + pad + col * cell_w, area.y + pad + row * cell_h);
            // How far the cell's middle is from `clear`, 0 inside it.
            let (mx, my) = (x + cell_w / 2, y + cell_h / 2);
            let dx = clear
                .x
                .saturating_sub(mx)
                .max(mx.saturating_sub(clear.x + clear.w));
            let dy = clear
                .y
                .saturating_sub(my)
                .max(my.saturating_sub(clear.y + clear.h));
            let t = ((dx * dx + dy * dy) as f32).sqrt() / fade;
            let t = t.min(1.0);
            // Smooth: slow at both ends, no edge where the fade stops.
            let strength = f32::from(mark.strength) * t * t * (3.0 - 2.0 * t);
            if strength < 2.0 {
                continue;
            }
            let color = blend(theme.background, theme.foreground, strength as u32);
            let mut text = [0u8; 4];
            let ch = mark.ch.encode_utf8(&mut text);
            draw_ui_text(pixels, width, font, ui, ch, (x, y), cell_w, color);
        }
    }
}

/// Kel's logo (see `logo`), its top left at `at`, a pixel of it `px` pixels
/// square: the letters in `letters`, the cursor in `cursor` (`None`: blinked off).
fn draw_logo(
    pixels: &mut [u32],
    width: usize,
    at: (usize, usize),
    px: usize,
    letters: u32,
    cursor: Option<u32>,
) {
    let px = px.max(1);
    for p in logo::pixels() {
        let color = if p.cursor { cursor } else { Some(letters) };
        if let Some(color) = color {
            let (x, y) = (at.0 + p.x * px, at.1 + p.y * px);
            fill_rect(pixels, width, x, y, px, px, color);
        }
    }
}

/// `text` in at most `chars` letters, cut at the start: a folder's end says the
/// most. e.g. "…\Documents\notes" in 12: "…ments\notes".
fn keep_end(text: &str, chars: usize) -> String {
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

/// A pixel of the project's name in the pixel font: 2 x 2 at 100%, like the icons.
fn project_name_px(ui: &Ui) -> usize {
    ui.px(2).max(1)
}

/// The project's name in the pixel font: its pixels and width (see `logo::word`).
/// None if it's off in the config, or a letter isn't in the font.
fn project_name_pixels(shown: &str, ui: &Ui) -> Option<(Vec<(usize, usize)>, usize)> {
    ui.pixel_name.then(|| logo::word(shown)).flatten()
}

/// How wide the project's name is in the tab bar: in pixels if the pixel font
/// has its letters, else in the UI's font.
fn project_name_width(shown: &str, ui: &Ui) -> usize {
    match project_name_pixels(shown, ui) {
        Some((_, w)) => w * project_name_px(ui),
        None => shown.chars().count() * ui.advance,
    }
}

/// The project's name as the tab bar shows it: at most `PROJECT_NAME_CHARS`
/// letters, e.g. "a-very-long-project-name" → "a-very-long-pro…".
fn project_name_shown(name: &str) -> String {
    if name.chars().count() <= PROJECT_NAME_CHARS {
        return name.to_string();
    }
    let mut shown: String = name.chars().take(PROJECT_NAME_CHARS - 1).collect();
    shown.push('…');
    shown
}

/// The search box, in the terminal panel's top right corner:
///
/// ```text
/// ┌────────────────────────────┐
/// │ > error|          3 of 17  │  what's typed, a cursor, which match
/// └────────────────────────────┘
/// ```
fn draw_search(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    search: &Search,
    font: &Font,
    ui: &mut Ui,
) {
    let area = layout.inside;
    let pad = ui.px(PALETTE_PADDING);
    let b = layout.border.max(1);
    let box_w = (SEARCH_CHARS * ui.advance + 2 * (pad + b)).min(area.w.saturating_sub(4 * pad));
    if box_w < 12 * ui.advance {
        return; // a window too narrow for it
    }
    let box_h = ui.line_h + 2 * (pad + b);
    let x = area.x + area.w - box_w - 2 * pad;
    let y = area.y + 2 * pad;
    let r = Rect {
        x,
        y,
        w: box_w,
        h: box_h,
    };
    outline(pixels, width, r, b, theme.border);
    fill_rect(
        pixels,
        width,
        x + b,
        y + b,
        box_w - 2 * b,
        box_h - 2 * b,
        theme.frame,
    );
    let text_w = box_w - 2 * (b + pad);
    let (text_x, text_y) = (x + b + pad, y + b + pad);

    // The status at the right end: dim, or red when nothing is found.
    let status = search.status().unwrap_or_default();
    let color = match search.current() {
        Some(_) => blend(theme.frame, theme.foreground, 128),
        None => theme.failed,
    };
    let status_w = (status.chars().count() * ui.advance).min(text_w / 2);
    let status_x = text_x + text_w - status_w;
    let at = (status_x, text_y);
    draw_ui_text(pixels, width, font, ui, &status, at, status_w, color);

    // What's typed, and a bar for the cursor after it.
    let query = format!("> {}", search.query());
    let query_w = text_w.saturating_sub(status_w + ui.advance);
    let at = (text_x, text_y);
    let typed = draw_ui_text(
        pixels,
        width,
        font,
        ui,
        &query,
        at,
        query_w,
        theme.foreground,
    );
    fill_rect(
        pixels,
        width,
        text_x + typed,
        text_y,
        ui.px(2),
        ui.line_h,
        theme.cursor,
    );
}

/// One line of UI text from (x, y), cut off where it would pass `max_w` pixels.
/// Returns how wide the drawn part is.
#[allow(clippy::too_many_arguments)]
fn draw_ui_text(
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
fn draw_ui_hint(
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

/// The theme glass makes, its background solid again (`solid_color`, or the
/// theme's): the frame around the panel stays glass.
fn solid(glass: &Glass, theme: &Theme) -> Theme {
    Theme {
        background: glass.solid_color.unwrap_or(theme.background),
        ..glass.theme(theme)
    }
}

/// `text` in at most `max` characters: the middle goes, for a `…`. e.g.
/// `https://example.com/a/b/c/page` in 20: `https:/…m/a/b/c/page`.
fn cut_middle(text: &str, max: usize) -> String {
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

/// A rectangle's outline, `t` pixels thick.
fn outline(pixels: &mut [u32], width: usize, r: Rect, t: usize, color: u32) {
    fill_rect(pixels, width, r.x, r.y, r.w, t, color); // top
    fill_rect(pixels, width, r.x, r.y + r.h - t, r.w, t, color); // bottom
    fill_rect(pixels, width, r.x, r.y, t, r.h, color); // left
    fill_rect(pixels, width, r.x + r.w - t, r.y, t, r.h, color); // right
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

/// Characters that only steer others and show nothing: the joiner in emoji
/// sequences (ZWJ, U+200D), FE0F ("as an emoji") and the like. Some fonts still
/// have a mark for them (Segoe UI Emoji's ZWJ: a bar 4 x 20 pixels). ConPTY gives
/// each a cell (see `char_width`), which stays empty.
fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}'
    )
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
fn glyph<'a>(
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
fn draw_underline(
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

/// Draws `icon` with its top-left corner at (x, y), its lit pixels in `color`.
/// Each of its pixels becomes a `k` x `k` square: at ×2, a 16x16 icon is 32x32.
fn draw_icon(
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
fn fill_circle(pixels: &mut [u32], width: usize, cx: f32, cy: f32, r: f32, hole: f32, color: u32) {
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

/// One cell's size, measured (see `measure_cell`).
#[derive(Clone, Copy)]
struct Measured {
    w: usize,
    h: usize,
    /// Pixels from the cell's top down to the baseline.
    ascent: i32,
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
fn measure_cell(font: &Font, size: f32, line_height: f32) -> anyhow::Result<Measured> {
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
fn fills_cell(ch: char) -> bool {
    matches!(ch, '\u{2500}'..='\u{259F}' | '\u{E0B0}'..='\u{E0BF}')
}

/// Mixes one color channel: `a` = 0 gives `bg`, 255 gives `fg`.
/// e.g. mix(0, 255, 128) = 128, halfway between.
fn mix(bg: u32, fg: u32, a: u32) -> u32 {
    (fg * a + bg * (255 - a)) / 255
}

/// Blends color `fg` over color `bg`, channel by channel. `a` is 0-255.
/// The top byte too: how clear a color is, with glass (see `glass`). So a
/// letter's edge over glass is part clear, like the glass.
fn blend(bg: u32, fg: u32, a: u32) -> u32 {
    let clear = mix(bg >> 24, fg >> 24, a);
    let r = mix((bg >> 16) & 0xff, (fg >> 16) & 0xff, a);
    let g = mix((bg >> 8) & 0xff, (fg >> 8) & 0xff, a);
    let b = mix(bg & 0xff, fg & 0xff, a);
    (clear << 24) | (r << 16) | (g << 8) | b
}

/// Draws a letter that fills its cell (see `fills_cell`) to the cell's edges.
/// Its middle as the font draws it (a `─` stays thin), its edge rows, half
/// covered, are their neighbors inside instead (no seam); and a cell taller than
/// the font's line (`line_height`) repeats them up and down, so `│` goes on.
fn draw_stretched(
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn blending_over_glass() {
        // A letter (solid) half over glass (51 of 255 clear): half as clear.
        let glass = crate::glass::see_through(0x1e1e1e, 204);
        assert_eq!(blend(glass, 0xd4d4d4, 255), 0x00d4_d4d4);
        assert_eq!(blend(glass, 0xd4d4d4, 0), glass);
        assert_eq!(blend(glass, 0xd4d4d4, 128) >> 24, 25);
        // Without glass, as before: the top byte stays 0.
        assert_eq!(blend(0x1e1e1e, 0xd4d4d4, 128) >> 24, 0);
    }

    #[test]
    fn a_long_hint_keeps_its_end() {
        assert_eq!(keep_end(r"C:\code\web", 20), r"C:\code\web");
        assert_eq!(keep_end(r"…\Documents\notes", 12), r"…ments\notes");
        assert_eq!(keep_end("abc", 0), "");
    }
}
