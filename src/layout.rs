//! Where things go in the window: plain rectangles, no pixels.
//!
//! The window holds the tab bar and a panel below it; the panel holds the terminal's text:
//!
//! ```text
//! ┌ window ─────────────────────────┐
//! │  margin                         │
//! │  ┌ tab ┐  tab     tab           │  ← tab bar, tab_height tall
//! │  ┌┘   └──────────────────────┐ │
//! │  │ panel (border)  padding    │ │
//! │  │  ┌ text: the grid ───────┐ │ │
//! │  │  └───────────────────────┘ │ │
//! │  └────────────────────────────┘ │
//! └─────────────────────────────────┘
//! ```
//!
//! The sizes come from the `[layout]` part of the config, so trying another look
//! means changing numbers there.
//!
//! With `tab_bar = "hover"` (or `"never"`) the panel takes the tab bar's place
//! too: a plain terminal, nothing above it. The bar then comes over the panel's
//! top when it's wanted (see `Layout::drawn`), and the text doesn't move.
//!
//! With `tab_bar = "title"` the panel takes the bar's place as well, and its
//! top is a title: one sheet, the text below a line that says where you are
//! (`myapp/nvim`), with no tabs.
//!
//! All sizes are for a 100% display. The layout multiplies them by the UI's scale
//! (see `scaled`): at Windows' 150%, a 32-pixel tab bar is 48 pixels tall.

use serde::Deserialize;
use winit::window::ResizeDirection;

/// How close to the window's edge the mouse must be to resize the window.
const RESIZE_EDGE: usize = 5;
/// How far from a corner, along both edges, resizing goes diagonally.
const RESIZE_CORNER: usize = 12;

/// A window button's width, like the Windows ones.
const BUTTON_WIDTH: usize = 46;
/// Space between the last tab and the window buttons.
const BUTTONS_GAP: usize = 8;
/// The `+` (new tab) and `⌄` (palette) buttons after the tabs: this wide each.
const BAR_BUTTON_WIDTH: usize = 28;
/// The sidebar: this wide, from the tab bar's top-left corner down.
const SIDEBAR_WIDTH: usize = 280;
/// A tab's `×`: a square this big, this far from the tab's right end.
const TAB_CLOSE_SIZE: usize = 18;
const TAB_CLOSE_INSET: usize = 7;

/// `n` pixels of a 100% display at `scale`, e.g. 32 at 1.5 → 48. Never thinner
/// than one pixel: a 1-pixel line at 0.75 stays 1, not 0.
pub fn scaled(n: usize, scale: f32) -> usize {
    if n == 0 {
        return 0;
    }
    ((n as f32 * scale).round() as usize).max(1)
}

/// The buttons in the top-right corner, in place of the Windows title bar's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

/// What the mouse is over in the title bar: drawn lit, and what a click there does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TitleHit {
    Window(WindowButton),
    /// `+`: a new tab.
    NewTab,
    /// `⌄`: the command palette.
    Palette,
    /// the sidebar button, before the tabs: the sidebar.
    Sidebar,
    /// The tab at `index`; `close`: on its `×`.
    Tab {
        index: usize,
        close: bool,
    },
    /// The title, where there are no tabs (`tab_bar = "title"`): the tab on
    /// screen's. It moves the window like the empty bar; a right click: its card.
    Title,
}

/// What's above the terminal: `tab_bar` in the config.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Bar {
    /// The tabs, above the panel.
    #[default]
    Always,
    /// No tabs: the panel's own top is a title, the project and the tab on
    /// screen in its middle (`myapp/nvim`). One sheet, and it moves the window.
    Title,
    /// Nothing. The tabs come over the panel's top while the mouse is at the
    /// window's top, for a moment as the tab on screen changes, and while Kel
    /// says something there.
    Hover,
    /// Nothing. The tabs come only for what Kel says.
    Never,
}

impl Bar {
    /// As the config names it.
    pub fn name(self) -> &'static str {
        match self {
            Bar::Always => "always",
            Bar::Title => "title",
            Bar::Hover => "hover",
            Bar::Never => "never",
        }
    }

    /// The bar comes and goes, over the panel's top.
    fn hidden(self) -> bool {
        matches!(self, Bar::Hover | Bar::Never)
    }
}

/// When a group of title bar buttons shows: `sidebar_button`, `tab_buttons` and
/// `window_buttons` in the config.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Show {
    #[default]
    Always,
    /// While the mouse is over the title bar. The place stays theirs, so the tabs
    /// don't move when they come; a click there works as ever.
    Hover,
    /// No button, and its place goes to the tabs.
    Never,
}

impl Show {
    /// Is it on screen, with the mouse over the title bar or not?
    pub fn shown(self, mouse_on_bar: bool) -> bool {
        match self {
            Show::Always => true,
            Show::Hover => mouse_on_bar,
            Show::Never => false,
        }
    }

    /// `width`, or none where the button isn't there at all.
    fn room(self, width: usize) -> usize {
        if self == Show::Never { 0 } else { width }
    }
}

/// A rectangle in pixels: top-left corner (x, y), width w, height h.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Rect {
    /// The same rectangle, `by` pixels smaller on every side.
    /// e.g. (0, 0, 100 x 50) shrunk by 8: (8, 8, 84 x 34).
    pub fn shrink(self, by: usize) -> Rect {
        Rect {
            x: self.x + by,
            y: self.y + by,
            w: self.w.saturating_sub(2 * by),
            h: self.h.saturating_sub(2 * by),
        }
    }

    /// Is the point (x, y) inside?
    pub fn contains(&self, x: usize, y: usize) -> bool {
        (self.x..self.x + self.w).contains(&x) && (self.y..self.y + self.h).contains(&y)
    }
}

/// The spaces around the terminal, in pixels: the `[layout]` part of the config.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Spacing {
    /// Between the window's edge and the panel.
    pub margin: usize,
    /// The panel's border line.
    pub border: usize,
    /// Between the panel's border and the text.
    pub padding: usize,
    /// The tab bar's height.
    pub tab_height: usize,
    /// What's above the terminal (see `Bar`).
    pub tab_bar: Bar,
    /// A tab's width. With many tabs, they get narrower to fit.
    pub tab_width: usize,
    /// The button before the tabs.
    pub sidebar_button: Show,
    /// `+` and `⌄` after the tabs.
    pub tab_buttons: Show,
    /// Minimize, maximize and close.
    pub window_buttons: Show,
    /// How round the corners are: the panel's, the tab's, the boxes over them.
    pub corner_radius: usize,
}

impl Default for Spacing {
    fn default() -> Self {
        Spacing {
            margin: 8,
            border: 0,
            padding: 6,
            tab_height: 32,
            tab_bar: Bar::Always,
            tab_width: 180,
            sidebar_button: Show::Hover,
            tab_buttons: Show::Hover,
            window_buttons: Show::Hover,
            corner_radius: 6,
        }
    }
}

/// Where everything is, for one window size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// The UI's scale: Windows' display scale times `ui_scale` from the config.
    /// The renderer sizes its own things (tab names, icons) by it too.
    pub scale: f32,
    /// The whole window.
    pub window: Rect,
    /// Everything above the panel, from the window's top edge: Kel's own title bar.
    /// Its empty parts move the window.
    pub title_bar: Rect,
    /// The part of the title bar where the tabs are.
    pub tab_bar: Rect,
    /// the sidebar button at the tab bar's start: opens the sidebar. The project's name and the
    /// tabs come after it.
    pub sidebar_button: Rect,
    /// The window buttons, top right, from the window's top edge down to the panel.
    pub minimize: Rect,
    pub maximize: Rect,
    pub close: Rect,
    /// The panel, with its border.
    pub panel: Rect,
    /// The panel inside its border: the terminal's background.
    pub inside: Rect,
    /// Where the grid of cells starts. Its size says how many cells fit.
    pub text: Rect,
    /// The panel's border line, in pixels (the active tab draws its own the same).
    pub border: usize,
    /// What's above the terminal (see `Bar`). Not `Always`: the panel is under
    /// `title_bar` and `tab_bar` too, which are where the tabs come, or the
    /// title is.
    pub bar_shows: Bar,
    /// When each group of buttons shows (see `Show`).
    pub sidebar_shows: Show,
    pub tab_buttons_show: Show,
    pub window_buttons_show: Show,
    /// How round the corners are, in pixels. 0: square.
    pub radius: usize,
    tab_width: usize,
    bar_button_w: usize,
    /// Room at the tab bar's start, before the first tab: the open project's name
    /// goes there (see `set_lead`). 0 without a project.
    lead: usize,
}

impl Spacing {
    /// The layout for a window of `width` x `height` pixels, at `scale`.
    pub fn layout(&self, width: usize, height: usize, scale: f32) -> Layout {
        let s = |n: usize| scaled(n, scale);
        let (margin, border, padding) = (s(self.margin), s(self.border), s(self.padding));
        let tab_height = s(self.tab_height);
        let window = Rect {
            x: 0,
            y: 0,
            w: width,
            h: height,
        };
        // Inside the margin: the tab bar on top, the panel below it.
        let area = window.shrink(margin);

        // The title bar: from the window's top edge to the panel.
        let title_bar = Rect {
            h: (margin + tab_height).min(height),
            ..window
        };
        // The buttons fill its right end, right up to the window's corner
        // (easy to hit: the mouse stops at the screen's edge).
        let button_w = self.window_buttons.room(s(BUTTON_WIDTH));
        let button = |n: usize| Rect {
            x: width.saturating_sub(n * button_w),
            w: button_w,
            ..title_bar
        };
        let (minimize, maximize, close) = (button(3), button(2), button(1));
        // The tabs get the rest, up to a small gap before the buttons.
        let tab_bar = Rect {
            w: minimize.x.saturating_sub(s(BUTTONS_GAP) + area.x),
            h: tab_height.min(area.h),
            ..area
        };
        // A tab bar that comes and goes is over the panel: the panel has its
        // room. A title is the panel's own top: the text starts below it.
        let panel = match self.tab_bar {
            Bar::Always => below(area, tab_bar.h),
            _ => area,
        };
        let inside = panel.shrink(border);
        let text = match self.tab_bar {
            Bar::Title => below(inside, tab_bar.h).shrink(padding),
            _ => inside.shrink(padding),
        };
        let sidebar_button = Rect {
            w: self.sidebar_button.room(s(BAR_BUTTON_WIDTH)).min(tab_bar.w),
            ..tab_bar
        };
        Layout {
            scale,
            window,
            title_bar,
            tab_bar,
            sidebar_button,
            minimize,
            maximize,
            close,
            panel,
            inside,
            text,
            border,
            bar_shows: self.tab_bar,
            sidebar_shows: self.sidebar_button,
            tab_buttons_show: self.tab_buttons,
            window_buttons_show: self.window_buttons,
            radius: s(self.corner_radius),
            tab_width: s(self.tab_width),
            // No tabs, no `+` and `⌄` after them.
            bar_button_w: match self.tab_bar {
                Bar::Title => 0,
                _ => self.tab_buttons.room(s(BAR_BUTTON_WIDTH)),
            },
            lead: 0,
        }
    }

    /// The window size that leaves exactly `text_w` x `text_h` pixels for the text.
    pub fn window_size(&self, text_w: usize, text_h: usize, scale: f32) -> (usize, usize) {
        let s = |n: usize| scaled(n, scale);
        let edge = s(self.margin) + s(self.border) + s(self.padding);
        let bar = match self.tab_bar {
            Bar::Always | Bar::Title => s(self.tab_height),
            _ => 0, // over the panel
        };
        (text_w + 2 * edge, text_h + 2 * edge + bar)
    }
}

/// `rect` without its top `h` pixels.
fn below(rect: Rect, h: usize) -> Rect {
    let h = h.min(rect.h);
    Rect {
        y: rect.y + h,
        h: rect.h - h,
        ..rect
    }
}

impl Layout {
    /// Which way the window resizes when dragged at (x, y): near an edge or a
    /// corner, `Some`. Without the Windows title bar, Kel decides this itself.
    pub fn resize_edge(&self, x: usize, y: usize) -> Option<ResizeDirection> {
        let (edge, corner_size) = (
            scaled(RESIZE_EDGE, self.scale),
            scaled(RESIZE_CORNER, self.scale),
        );
        // How far the mouse is from each edge, in pixels.
        let left = x;
        let top = y;
        let right = self.window.w.saturating_sub(x + 1);
        let bottom = self.window.h.saturating_sub(y + 1);
        // Close to one edge, and not far along the other: a corner.
        let corner =
            |a: usize, b: usize| a < corner_size && b < corner_size && (a < edge || b < edge);
        use ResizeDirection::*;
        Some(match () {
            _ if corner(top, left) => NorthWest,
            _ if corner(top, right) => NorthEast,
            _ if corner(bottom, left) => SouthWest,
            _ if corner(bottom, right) => SouthEast,
            _ if top < edge => North,
            _ if bottom < edge => South,
            _ if left < edge => West,
            _ if right < edge => East,
            _ => return None,
        })
    }

    /// The layout as it's drawn now. A tab bar that comes and goes (`bar_shows`
    /// isn't `Always`), up: the panel starts below it, as with a bar that's
    /// always there, so the two look the same. The text stays where it is (no
    /// program has to draw again): its rows under the bar aren't drawn.
    pub fn drawn(&self, bar_up: bool) -> Layout {
        if !self.bar_shows.hidden() || !bar_up {
            return *self;
        }
        let panel = below(self.panel, self.tab_bar.h);
        Layout {
            panel,
            inside: panel.shrink(self.border),
            ..*self
        }
    }

    /// The layout for what's over the terminal's text (the start screen, the
    /// search, the palette): under a title, `inside` starts below it, so the
    /// title stays in sight.
    pub fn body(&self) -> Layout {
        match self.bar_shows {
            Bar::Title => Layout {
                inside: below(self.inside, self.tab_bar.h),
                ..*self
            },
            _ => *self,
        }
    }

    /// Where the title is (`tab_bar = "title"`): a tab's width, in the middle
    /// of the panel. What's written can be longer; this much of it takes the
    /// right click, and a tab's card hangs from it.
    pub fn title(&self) -> Rect {
        let w = self.tab_width.min(self.tab_bar.w);
        let middle = self.panel.x + self.panel.w / 2;
        let last = (self.tab_bar.x + self.tab_bar.w).saturating_sub(w);
        Rect {
            x: middle
                .saturating_sub(w / 2)
                .clamp(self.tab_bar.x, last.max(self.tab_bar.x)),
            w,
            ..self.tab_bar
        }
    }

    /// What the card of the tab at `index` (of `count`) hangs from: its tab, or
    /// the title where there are no tabs.
    pub fn tab_anchor(&self, index: usize, count: usize) -> Rect {
        let tabs = self.tabs(count);
        tabs.get(index).copied().unwrap_or_else(|| self.title())
    }

    /// The mouse at height `y` calls a hidden tab bar: above the text, where
    /// there's nothing to click or select. Without spaces around the text
    /// (`margin` and `padding` 0): the window's top few pixels.
    pub fn calls_bar(&self, y: usize) -> bool {
        y < self.text.y.max(scaled(4, self.scale))
    }

    /// The window button at (x, y), if there is one.
    pub fn button_at(&self, x: usize, y: usize) -> Option<WindowButton> {
        [
            (self.minimize, WindowButton::Minimize),
            (self.maximize, WindowButton::Maximize),
            (self.close, WindowButton::Close),
        ]
        .into_iter()
        .find(|(rect, _)| rect.contains(x, y))
        .map(|(_, button)| button)
    }

    /// How round the small things are: a chosen line, a button under the mouse.
    pub fn small_radius(&self) -> usize {
        self.radius / 2
    }

    /// Leaves `lead` pixels after the sidebar button, before the first tab, for the project's
    /// name. At most half the bar: the tabs need room too.
    pub fn set_lead(&mut self, lead: usize) {
        self.lead = lead.min(self.tab_bar.w / 2);
    }

    /// The sidebar's panel: over the left of the window, from the tab bar's top
    /// to as far from the bottom, `SIDEBAR_WIDTH` wide (less in a narrow window).
    pub fn sidebar(&self) -> Rect {
        let (x, y) = (self.tab_bar.x, self.tab_bar.y);
        Rect {
            x,
            y,
            w: scaled(SIDEBAR_WIDTH, self.scale).min(self.window.w.saturating_sub(2 * x)),
            h: self.window.h.saturating_sub(2 * y),
        }
    }

    /// Where the first tab starts: after the sidebar button and the project's name.
    fn tabs_start(&self) -> usize {
        self.sidebar_button.x + self.sidebar_button.w + self.lead
    }

    /// Where each of `count` tabs is, left to right in the tab bar. They leave room
    /// for the sidebar button before them and the `+` and `⌄` buttons after them. e.g. an
    /// 846-pixel bar: 3 tabs are 180 wide each; 10 tabs get (846 - 3 x 28) / 10
    /// = 76 each.
    pub fn tabs(&self, count: usize) -> Vec<Rect> {
        if self.bar_shows == Bar::Title {
            return Vec::new(); // a title, no tabs
        }
        let w = self.tab_w(count);
        (0..count)
            .map(|i| Rect {
                x: self.tabs_start() + i * w,
                w,
                ..self.tab_bar
            })
            .collect()
    }

    /// The place among `count` tabs for a tab dragged to `x`: the one under it,
    /// or the first or last past the ends. All tabs are as wide, so it's a
    /// division: x = 86 + 400 with 180-wide tabs from 86 → place 2.
    pub fn tab_slot(&self, x: usize, count: usize) -> usize {
        let start = self.tabs_start();
        let w = self.tab_w(count).max(1);
        (x.saturating_sub(start) / w).min(count.saturating_sub(1))
    }

    /// One tab's width, when there are `count`.
    fn tab_w(&self, count: usize) -> usize {
        let before = self.tabs_start() - self.tab_bar.x;
        let room = self
            .tab_bar
            .w
            .saturating_sub(before + 2 * self.bar_button_w);
        self.tab_width.min(room / count.max(1))
    }

    /// The `+` and `⌄` buttons, right after the last of `count` tabs.
    pub fn bar_buttons(&self, count: usize) -> (Rect, Rect) {
        let x = self.tabs_start() + count * self.tab_w(count);
        let button = |i: usize| Rect {
            x: x + i * self.bar_button_w,
            w: self.bar_button_w,
            ..self.tab_bar
        };
        (button(0), button(1))
    }

    /// The `×` of a tab at `tab`: a square at its right end, in the middle from
    /// top to bottom.
    pub fn tab_close(&self, tab: Rect) -> Rect {
        let size = scaled(TAB_CLOSE_SIZE, self.scale);
        let inset = scaled(TAB_CLOSE_INSET, self.scale);
        Rect {
            x: (tab.x + tab.w).saturating_sub(inset + size),
            y: tab.y + tab.h.saturating_sub(size) / 2,
            w: size,
            h: size,
        }
    }

    /// What's at (x, y) in the title bar, with `count` tabs open. `None`: the
    /// empty bar, which moves the window.
    pub fn title_hit(&self, x: usize, y: usize, count: usize) -> Option<TitleHit> {
        if let Some(button) = self.button_at(x, y) {
            return Some(TitleHit::Window(button));
        }
        if self.sidebar_button.contains(x, y) {
            return Some(TitleHit::Sidebar);
        }
        if self.bar_shows == Bar::Title {
            return self.title().contains(x, y).then_some(TitleHit::Title);
        }
        let tabs = self.tabs(count);
        if let Some(index) = tabs.iter().position(|tab| tab.contains(x, y)) {
            let close = self.tab_close(tabs[index]).contains(x, y);
            return Some(TitleHit::Tab { index, close });
        }
        let (new_tab, palette) = self.bar_buttons(count);
        if new_tab.contains(x, y) {
            Some(TitleHit::NewTab)
        } else if palette.contains(x, y) {
            Some(TitleHit::Palette)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_buttons() {
        let plain = Spacing::default().layout(1000, 700, 1.0);
        // Hover: the same places, nothing moves when they come.
        let hover = Spacing {
            sidebar_button: Show::Hover,
            tab_buttons: Show::Hover,
            window_buttons: Show::Hover,
            ..Spacing::default()
        };
        let layout = hover.layout(1000, 700, 1.0);
        assert_eq!(layout.tabs(2), plain.tabs(2));
        assert_eq!(layout.close, plain.close);
        assert!(layout.window_buttons_show.shown(true));
        assert!(!layout.window_buttons_show.shown(false));
        // Never: their places go to the tabs, and a click there finds nothing.
        let never = Spacing {
            sidebar_button: Show::Never,
            tab_buttons: Show::Never,
            window_buttons: Show::Never,
            ..Spacing::default()
        };
        let layout = never.layout(1000, 700, 1.0);
        assert_eq!(layout.tabs(1)[0].x, 8);
        assert_eq!(layout.tab_bar.w, 1000 - 8 - 8);
        assert_eq!(layout.title_hit(995, 20, 1), None);
        assert_eq!(layout.title_hit(190, 20, 1), None); // where + would be
    }

    #[test]
    fn a_tab_bar_that_comes_and_goes() {
        let spacing = Spacing {
            tab_bar: Bar::Hover,
            ..Spacing::default()
        };
        let layout = spacing.layout(1000, 700, 1.0);
        // The panel has the bar's room: it starts at the margin, 8.
        assert_eq!((layout.panel.y, layout.panel.h), (8, 684));
        assert_eq!(layout.text.y, 8 + 6);
        // The bar's place is the same as ever: over the panel's top.
        assert_eq!(
            layout.tab_bar,
            Spacing::default().layout(1000, 700, 1.0).tab_bar
        );
        // Down: drawn as it is. Up: the panel starts below the bar, the text stays.
        assert_eq!(layout.drawn(false), layout);
        let up = layout.drawn(true);
        assert_eq!((up.panel.y, up.panel.h), (40, 652));
        assert_eq!(up.inside.y, 40);
        assert_eq!(up.text, layout.text);
        // A bar that's always there is never over the panel.
        let always = Spacing::default().layout(1000, 700, 1.0);
        assert_eq!(always.drawn(true), always);
        // The mouse above the text calls it: 14 pixels here, 4 with no spaces.
        assert!(layout.calls_bar(13));
        assert!(!layout.calls_bar(14));
        let bare = Spacing {
            margin: 0,
            padding: 0,
            ..spacing.clone()
        };
        assert!(bare.layout(1000, 700, 1.0).calls_bar(3));
        // The window is as tall as the text and its spaces: no bar's height.
        let (w, h) = spacing.window_size(800, 480, 1.0);
        assert_eq!((w, h), (800 + 28, 480 + 28));
    }

    #[test]
    fn a_title_in_place_of_the_tabs() {
        let spacing = Spacing {
            tab_bar: Bar::Title,
            ..Spacing::default()
        };
        let layout = spacing.layout(1000, 700, 1.0);
        let tabs = Spacing::default().layout(1000, 700, 1.0);
        // The panel is the whole sheet; the text is where it is with tabs.
        assert_eq!((layout.panel.y, layout.panel.h), (8, 684));
        assert_eq!(layout.text, tabs.text);
        assert_eq!(
            spacing.window_size(800, 480, 1.0),
            Spacing::default().window_size(800, 480, 1.0)
        );
        // Never over the panel: it's there.
        assert_eq!(layout.drawn(true), layout);
        // What's over the text starts below the title.
        assert_eq!(layout.body().inside.y, 40);
        assert_eq!(tabs.body(), tabs);
        // No tabs, no + and ⌄: the title in the middle (180 wide, around 500),
        // the empty bar beside it, the sidebar's and the window's buttons.
        assert!(layout.tabs(3).is_empty());
        assert_eq!(layout.title_hit(500, 20, 3), Some(TitleHit::Title));
        assert_eq!(layout.title_hit(100, 20, 3), None);
        assert_eq!(layout.title_hit(700, 20, 3), None);
        assert_eq!(layout.title_hit(20, 20, 3), Some(TitleHit::Sidebar));
        assert_eq!(
            layout.title_hit(990, 20, 3),
            Some(TitleHit::Window(WindowButton::Close))
        );
        // A tab's card hangs from the title; with tabs, from its tab.
        assert_eq!(layout.tab_anchor(2, 3), layout.title());
        assert_eq!(tabs.tab_anchor(2, 3), tabs.tabs(3)[2]);
    }

    #[test]
    fn tab_bar_panel_and_text_inside_the_window() {
        let spacing = Spacing {
            border: 1,
            ..Spacing::default()
        };
        let layout = spacing.layout(1000, 700, 1.0);
        // margin 8, then the tab bar, 32 tall. It ends 8 pixels before the
        // 3 buttons (3 x 46 = 138): 1000 - 138 - 8 - 8 = 846 wide.
        assert_eq!(
            layout.tab_bar,
            Rect {
                x: 8,
                y: 8,
                w: 846,
                h: 32
            }
        );
        // The close button: the top-right corner, down to the panel (8 + 32 = 40).
        assert_eq!(
            layout.close,
            Rect {
                x: 954,
                y: 0,
                w: 46,
                h: 40
            }
        );
        // the panel right below it: y = 8 + 32 = 40
        assert_eq!(
            layout.panel,
            Rect {
                x: 8,
                y: 40,
                w: 984,
                h: 652
            }
        );
        // + border 1 + padding 6 = 7 more on every side
        assert_eq!(
            layout.text,
            Rect {
                x: 15,
                y: 47,
                w: 970,
                h: 638
            }
        );
    }

    #[test]
    fn everything_bigger_at_150_percent() {
        let spacing = Spacing {
            border: 1,
            ..Spacing::default()
        };
        let layout = spacing.layout(1500, 1050, 1.5);
        // margin 8 → 12, tab bar 32 → 48, buttons 46 → 69 wide
        assert_eq!((layout.tab_bar.y, layout.tab_bar.h), (12, 48));
        assert_eq!(layout.close.w, 69);
        // border 1 → 2 (1.5 rounds up), padding 6 → 9: text at 12 + 2 + 9 = 23
        assert_eq!(layout.text.x, 23);
        assert_eq!(layout.tabs(1)[0].w, 270); // 180 → 270
    }

    #[test]
    fn scaled_lines_stay_visible() {
        assert_eq!(scaled(1, 0.75), 1);
        assert_eq!(scaled(0, 2.0), 0);
        assert_eq!(scaled(32, 1.25), 40);
    }

    #[test]
    fn tabs_side_by_side() {
        let layout = Spacing::default().layout(1000, 700, 1.0);
        let tabs = layout.tabs(3);
        // After the sidebar button, 28 wide.
        assert_eq!(
            tabs[0],
            Rect {
                x: 36,
                y: 8,
                w: 180,
                h: 32
            }
        );
        assert_eq!(tabs[2].x, 36 + 2 * 180);
        // Too many for 180 each: (846 - 3 x 28 for the sidebar button, + and ⌄) / 10 = 76
        assert_eq!(layout.tabs(10)[0].w, 76);
        // A click at (250, 20) is on the second tab.
        assert!(tabs[1].contains(250, 20));
        assert!(!tabs[1].contains(250, 45)); // below the bar: the panel
    }

    #[test]
    fn resize_edges() {
        let layout = Spacing::default().layout(1000, 700, 1.0);
        assert_eq!(layout.resize_edge(0, 300), Some(ResizeDirection::West));
        assert_eq!(layout.resize_edge(999, 300), Some(ResizeDirection::East));
        assert_eq!(layout.resize_edge(500, 698), Some(ResizeDirection::South));
        assert_eq!(layout.resize_edge(500, 2), Some(ResizeDirection::North));
        // Near the corner: 2 from the top, 8 from the right, diagonal.
        assert_eq!(layout.resize_edge(991, 2), Some(ResizeDirection::NorthEast));
        // Further in: the close button, not an edge.
        assert_eq!(layout.resize_edge(980, 20), None);
    }

    #[test]
    fn window_buttons() {
        let layout = Spacing::default().layout(1000, 700, 1.0);
        assert_eq!(layout.button_at(990, 5), Some(WindowButton::Close));
        assert_eq!(layout.button_at(940, 20), Some(WindowButton::Maximize));
        assert_eq!(layout.button_at(870, 20), Some(WindowButton::Minimize));
        assert_eq!(layout.button_at(500, 20), None); // the tab bar
        assert_eq!(layout.button_at(990, 50), None); // the panel
    }

    #[test]
    fn title_bar_hits() {
        let layout = Spacing::default().layout(1000, 700, 1.0);
        // the sidebar button at 8..36, 2 tabs: 36..216 and 216..396, then + at 396..424 and ⌄
        // at 424..452.
        let hit = |x| layout.title_hit(x, 20, 2);
        assert_eq!(hit(20), Some(TitleHit::Sidebar));
        assert_eq!(
            hit(100),
            Some(TitleHit::Tab {
                index: 0,
                close: false
            })
        );
        // The × of the second tab: 18 wide, ending 7 before the tab's end (396).
        assert_eq!(
            hit(380),
            Some(TitleHit::Tab {
                index: 1,
                close: true
            })
        );
        assert_eq!(hit(410), Some(TitleHit::NewTab));
        assert_eq!(hit(440), Some(TitleHit::Palette));
        assert_eq!(hit(600), None); // the empty bar
        assert_eq!(hit(990), Some(TitleHit::Window(WindowButton::Close)));
        // Many tabs: the buttons still fit before the window buttons.
        let (_, palette) = layout.bar_buttons(30);
        assert!(palette.x + palette.w <= layout.tab_bar.x + layout.tab_bar.w);
    }

    #[test]
    fn the_project_name_goes_before_the_tabs() {
        let mut layout = Spacing::default().layout(1000, 700, 1.0);
        layout.set_lead(50);
        let tabs = layout.tabs(2);
        assert_eq!(tabs[0].x, 8 + 28 + 50);
        assert_eq!(tabs[1].x, tabs[0].x + tabs[0].w);
        let (new_tab, _) = layout.bar_buttons(2);
        assert_eq!(new_tab.x, tabs[1].x + tabs[1].w);
        // A click on the name: the empty bar, which moves the window.
        assert_eq!(layout.title_hit(60, 20, 2), None);
        // Many tabs still fit, with the name.
        let (_, palette) = layout.bar_buttons(30);
        assert!(palette.x + palette.w <= layout.tab_bar.x + layout.tab_bar.w);
    }

    #[test]
    fn a_dragged_tab_s_place() {
        let mut layout = Spacing::default().layout(1000, 700, 1.0);
        layout.set_lead(50);
        let tabs = layout.tabs(3);
        assert_eq!(layout.tab_slot(tabs[0].x + 10, 3), 0);
        assert_eq!(layout.tab_slot(tabs[2].x, 3), 2);
        assert_eq!(layout.tab_slot(tabs[1].x + tabs[1].w - 1, 3), 1);
        // Past the ends: the first and the last place.
        assert_eq!(layout.tab_slot(0, 3), 0);
        assert_eq!(layout.tab_slot(990, 3), 2);
    }

    #[test]
    fn the_sidebar_panel() {
        let layout = Spacing::default().layout(1000, 700, 1.0);
        let panel = layout.sidebar();
        assert_eq!((panel.x, panel.y, panel.w, panel.h), (8, 8, 280, 684));
        // A narrow window: not wider than it.
        let narrow = Spacing::default().layout(200, 300, 1.0).sidebar();
        assert_eq!(narrow.w, 184);
    }

    #[test]
    fn window_size_fits_the_text() {
        let spacing = Spacing::default();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (w, h) = spacing.window_size(800, 480, scale);
            let text = spacing.layout(w, h, scale).text;
            assert_eq!((text.w, text.h), (800, 480), "at {scale}");
        }
    }

    #[test]
    fn tiny_window_gives_empty_text() {
        let layout = Spacing::default().layout(20, 20, 1.0);
        assert_eq!((layout.text.w, layout.text.h), (0, 0));
    }
}
