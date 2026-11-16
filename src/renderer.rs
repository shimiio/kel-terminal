//! Draws the grid into the window, pixel by pixel, on the CPU.
//!
//! - softbuffer gives us the window's pixels as one long `[u32]` list, row after row.
//!   Pixel (x, y) is at index `y * width + x`. A color is `0xRRGGBB`.
//! - `font` turns a character into a coverage map: how much of each pixel the
//!   letter covers, 0 (not at all) to 255 (fully).
//! - Every frame redraws everything: backgrounds, letters, cursor.
//!
//! Here: the `Renderer` with its settings, and `draw`, a whole frame: the
//! terminal's cells, then what's over them. The parts are in `renderer/`:
//!
//! - `bar`: the tab bar, its buttons, the project's name, the status line.
//! - `overlays`: the palette, the sidebar, a card, the search.
//! - `screens`: the start screen (its backdrop, the logo) and the help.
//! - `text`: glyphs, the UI's text, underlines, the size of a cell.
//! - `shapes`: rectangles, round corners, circles, icons, mixing colors.

mod bar;
mod overlays;
mod screens;
mod shapes;
mod text;

use crate::app::Selection;
use crate::card::Card;
use crate::fallback::Fallbacks;
use crate::font::{self, Font, Fonts, Metrics};
use crate::glass::Glass;
use crate::help::Help;
use crate::icons::Icons;
use crate::layout::{Bar, Layout, Rect, TitleHit, scaled};
use crate::palette::Palette;
use crate::search::{self, Search};
use crate::sidebar::Sidebar;
use crate::start::Start;
use crate::term::{Cell, CursorShape, Found, Grid, Past, Underline, Width};
use crate::theme::{Color, Theme};
use bar::{
    draw_bar_buttons, draw_status, draw_tab_bar, draw_title, draw_window_buttons,
    project_name_shown, project_name_width,
};
use overlays::{draw_card, draw_palette, draw_search, draw_sidebar, draw_switcher, switcher_place};
use screens::{draw_help, draw_start};
use shapes::{blend, fill_rect, fill_round, invisible, outline, solid};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use text::{
    Measured, cut_middle, draw_glyph, draw_stretched, draw_underline, fills_cell, glyph,
    measure_cell,
};
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
/// The tab's name in a title (`tab_bar = "title"`): at most this many letters.
const TITLE_CHARS: usize = 40;

/// How dark the window gets behind the open sidebar: this much black (of 255).
const SIDEBAR_DIM: u32 = 120;

/// What's over the terminal without a line around it (`border = 0`) stands out by
/// its color: the frame's, with this much black (of 255). Over glass that's denser
/// glass too, so the text on it reads better.
const PANE_BLACK: u32 = 130;

/// The search box: this wide, in letters (less in a narrow window).
const SEARCH_CHARS: usize = 36;
/// How much of the dim yellow the other matches get over their background (of 255).
const MATCH_TINT: u32 = 100;
/// How much of its color text from last time keeps (of 255), the rest is its
/// background's: a quarter fainter than today's, its colors still told apart.
const OLD_TEXT: u32 = 192;
/// And the line that ends it, with its day and time: less, it's not to be read.
const RULE: u32 = 110;
/// How much of the text's color lights the panel when the bell flashes, out of 255.
const FLASH: u32 = 40;

/// The command palette: at most this wide (in letters) and this many lines.
const PALETTE_CHARS: usize = 64;
/// The list of tabs (Ctrl+Tab): as wide as this many letters, and at most this
/// many tabs at a time.
const SWITCHER_CHARS: usize = 36;
const SWITCHER_LINES: usize = 12;
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
    /// The mouse is over the title bar: the buttons set to `hover` show.
    pub mouse_on_bar: bool,
    /// The list of tabs shows (Ctrl held after Ctrl+Tab, where the tabs aren't
    /// in sight): `tabs`, the one on screen chosen.
    pub switcher: bool,
    /// The tab bar shows. Always, unless the config hides it (`tab_bar`): then
    /// only while it's wanted, over the panel's top (see `Layout::drawn`).
    pub bar_up: bool,
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
            pixel_name: false,
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

    /// The tab whose line in the list of tabs (see `draw_switcher`) is at
    /// (x, y), with `count` tabs and the one at `active` on screen. `layout`:
    /// as it's drawn now.
    pub fn switcher_tab_at(
        &self,
        layout: &Layout,
        (count, active): (usize, usize),
        (x, y): (usize, usize),
    ) -> Option<usize> {
        let place = switcher_place(&layout.body(), &self.ui, count, active)?;
        place.tab_at(layout.border, x, y)
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
        // A tab bar that comes and goes, up: the panel starts below it.
        let layout = &layout.drawn(chrome.bar_up);
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
        // The tab on screen grows out of the panel: over the panel's corner, that
        // corner stays square.
        let r = layout.radius;
        let tabs = layout.tabs(chrome.tabs.len());
        let under_tab = chrome.bar_up
            && tabs
                .get(chrome.active)
                .is_some_and(|tab| tab.x < panel.x + r);
        let corners = [if under_tab { 0 } else { r }, r, r, r];
        if layout.border > 0 {
            fill_round(&mut buffer, width, panel, corners, theme.border);
        }
        let inner = corners.map(|n| n.saturating_sub(layout.border));
        fill_round(&mut buffer, width, inside, inner, theme.background);
        // After the panel: the tab on screen covers a piece of the panel's top line.
        let font = &self.ui_font; // all of the UI below: the built-in font
        let titled = layout.bar_shows == Bar::Title;
        if titled {
            draw_title(
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
        } else if chrome.bar_up {
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
        }
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
        // No bar, nowhere to say it: a question or a problem brings the bar
        // (see `App::bar_up`), a link's address doesn't.
        if let Some((text, color)) = message.filter(|_| chrome.bar_up) {
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
        // Its row in the view: lower, under output from last time (`Grid::above`).
        let cursor_row = cursor_row + grid.above();
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
                // Under a tab bar that came over the panel's top.
                if y < inside.y {
                    continue;
                }
                // The row that ends a last time: a line across, and its text
                // (`14 Nov 18:32`, kept at the row's start) near the right end,
                // a cell of space on each side: `───── 14 Nov 18:32 ─`.
                let rule = grid.visible_rule(row).map(|len| {
                    let start = grid.cols().saturating_sub(len + 2);
                    (start, start + len)
                });
                for col in 0..grid.cols() {
                    let x = text.x + col * cell_w;

                    let selected = selection.is_some_and(|s| s.contains(top + row, col));
                    let cell = match rule {
                        None => grid.visible_cell(row, col),
                        Some((start, end)) if (start..end).contains(&col) => {
                            grid.visible_cell(row, col - start)
                        }
                        Some(_) => Cell::default(),
                    };
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
                    // A background the program picked: glass too, if the config
                    // says so. Not an inverted one: that's its highlight.
                    let picked = cell.bg != Color::Default && !cell.inverse && !chrome.solid;
                    let bg = match picked {
                        true => self.glass.picked_background(bg),
                        false => bg,
                    };
                    let (fg, bg) = if cell.inverse { (bg, fg) } else { (fg, bg) };
                    // Dim text (`ESC[2m`, a shell's ghost suggestion): half way
                    // to its background.
                    let fg = if cell.dim { blend(bg, fg, 128) } else { fg };
                    // Text from last time, a little fainter; its rule, more.
                    let fg = match (cell.past, rule) {
                        (_, Some(_)) => blend(bg, fg, RULE),
                        (Past::Text, None) => blend(bg, fg, OLD_TEXT),
                        _ => fg,
                    };
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

                    // The rule's line: in the row's middle, not through its
                    // text or the cell on each side of it.
                    if let Some((start, end)) = rule
                        && (col + 1 < start || col > end)
                    {
                        let line = text_px(1).max(1);
                        let y = y + (cell_h - line) / 2;
                        fill_rect(&mut buffer, width, x, y, cell_w, line, fg);
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
        // What's over the text leaves a title in sight.
        let body = &layout.body();
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
                body,
                start,
                on,
                font,
                &mut self.ui,
            );
        }
        if let Some(search) = chrome.search {
            draw_search(&mut buffer, width, theme, body, search, font, &mut self.ui);
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
        if chrome.switcher {
            draw_switcher(&mut buffer, width, theme, body, chrome, font, &mut self.ui);
        }
        // Last: the palette is on top of everything.
        if let Some(palette) = chrome.palette {
            // The project's color (or the theme's accent): its chosen line's.
            let color = chrome.project.and_then(|(_, color)| color).or(theme.accent);
            draw_palette(
                &mut buffer,
                width,
                theme,
                body,
                (palette, color),
                font,
                &mut self.ui,
            );
        }
        // Except the help, which closes the others when it opens.
        if let Some(help) = chrome.help {
            draw_help(&mut buffer, width, theme, body, help, font, &mut self.ui);
        }

        if self.glass.on() {
            self.glass.finish(&mut buffer, width);
        }
        buffer.present().unwrap();
    }
}
