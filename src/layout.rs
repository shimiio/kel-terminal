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

use serde::Deserialize;

/// A window button's width, like the Windows ones.
const BUTTON_WIDTH: usize = 46;
/// Space between the last tab and the window buttons.
const BUTTONS_GAP: usize = 8;

/// The buttons in the top-right corner, in place of the Windows title bar's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowButton {
    Minimize,
    Maximize,
    Close,
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
    /// A tab's width. With many tabs, they get narrower to fit.
    pub tab_width: usize,
}

impl Default for Spacing {
    fn default() -> Self {
        Spacing {
            margin: 8,
            border: 1,
            padding: 6,
            tab_height: 32,
            tab_width: 180,
        }
    }
}

/// Where everything is, for one window size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Layout {
    /// Everything above the panel, from the window's top edge: Kel's own title bar.
    /// Its empty parts move the window.
    pub title_bar: Rect,
    /// The part of the title bar where the tabs are.
    pub tab_bar: Rect,
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
    tab_width: usize,
}

impl Spacing {
    /// The layout for a window of `width` x `height` pixels.
    pub fn layout(&self, width: usize, height: usize) -> Layout {
        let window = Rect {
            x: 0,
            y: 0,
            w: width,
            h: height,
        };
        // Inside the margin: the tab bar on top, the panel below it.
        let area = window.shrink(self.margin);

        // The title bar: from the window's top edge to the panel.
        let title_bar = Rect {
            h: (self.margin + self.tab_height).min(height),
            ..window
        };
        // The buttons fill its right end, right up to the window's corner
        // (easy to hit: the mouse stops at the screen's edge).
        let button = |n: usize| Rect {
            x: width.saturating_sub(n * BUTTON_WIDTH),
            w: BUTTON_WIDTH,
            ..title_bar
        };
        let (minimize, maximize, close) = (button(3), button(2), button(1));
        // The tabs get the rest, up to a small gap before the buttons.
        let tab_bar = Rect {
            w: minimize.x.saturating_sub(BUTTONS_GAP + area.x),
            h: self.tab_height.min(area.h),
            ..area
        };
        let panel = Rect {
            y: area.y + tab_bar.h,
            h: area.h - tab_bar.h,
            ..area
        };
        let inside = panel.shrink(self.border);
        let text = inside.shrink(self.padding);
        Layout {
            title_bar,
            tab_bar,
            minimize,
            maximize,
            close,
            panel,
            inside,
            text,
            border: self.border,
            tab_width: self.tab_width,
        }
    }

    /// The window size that leaves exactly `text_w` x `text_h` pixels for the text.
    pub fn window_size(&self, text_w: usize, text_h: usize) -> (usize, usize) {
        let edge = self.margin + self.border + self.padding;
        (text_w + 2 * edge, text_h + 2 * edge + self.tab_height)
    }
}

impl Layout {
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

    /// Where each of `count` tabs is, left to right in the tab bar.
    /// e.g. an 846-pixel bar: 3 tabs are 180 wide each; 10 tabs get 84 each.
    pub fn tabs(&self, count: usize) -> Vec<Rect> {
        if count == 0 {
            return Vec::new();
        }
        let w = self.tab_width.min(self.tab_bar.w / count);
        (0..count)
            .map(|i| Rect {
                x: self.tab_bar.x + i * w,
                w,
                ..self.tab_bar
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_bar_panel_and_text_inside_the_window() {
        let layout = Spacing::default().layout(1000, 700);
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
    fn tabs_side_by_side() {
        let layout = Spacing::default().layout(1000, 700);
        let tabs = layout.tabs(3);
        assert_eq!(
            tabs[0],
            Rect {
                x: 8,
                y: 8,
                w: 180,
                h: 32
            }
        );
        assert_eq!(tabs[2].x, 8 + 2 * 180);
        // Too many for 180 each: 846 / 10 = 84
        assert_eq!(layout.tabs(10)[0].w, 84);
        // A click at (200, 20) is on the second tab.
        assert!(tabs[1].contains(200, 20));
        assert!(!tabs[1].contains(200, 45)); // below the bar: the panel
    }

    #[test]
    fn window_buttons() {
        let layout = Spacing::default().layout(1000, 700);
        assert_eq!(layout.button_at(990, 5), Some(WindowButton::Close));
        assert_eq!(layout.button_at(940, 20), Some(WindowButton::Maximize));
        assert_eq!(layout.button_at(870, 20), Some(WindowButton::Minimize));
        assert_eq!(layout.button_at(500, 20), None); // the tab bar
        assert_eq!(layout.button_at(990, 50), None); // the panel
    }

    #[test]
    fn window_size_fits_the_text() {
        let spacing = Spacing::default();
        let (w, h) = spacing.window_size(800, 480);
        assert_eq!(spacing.layout(w, h).text.w, 800);
        assert_eq!(spacing.layout(w, h).text.h, 480);
    }

    #[test]
    fn tiny_window_gives_empty_text() {
        let layout = Spacing::default().layout(20, 20);
        assert_eq!((layout.text.w, layout.text.h), (0, 0));
    }
}
