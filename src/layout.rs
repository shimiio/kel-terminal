//! Where things go in the window: plain rectangles, no pixels.
//!
//! The window holds a panel, and the panel holds the terminal's text:
//!
//! ```text
//! ┌ window ─────────────────────────┐
//! │  margin                         │
//! │  ┌ panel (border) ────────────┐ │
//! │  │  padding                   │ │
//! │  │  ┌ text: the grid ───────┐ │ │
//! │  │  └───────────────────────┘ │ │
//! │  └────────────────────────────┘ │
//! └─────────────────────────────────┘
//! ```
//!
//! The sizes come from the `[layout]` part of the config, so trying another look
//! means changing numbers there.

use serde::Deserialize;

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
}

impl Default for Spacing {
    fn default() -> Self {
        Spacing {
            margin: 8,
            border: 1,
            padding: 6,
        }
    }
}

/// Where everything is, for one window size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Layout {
    /// The panel, with its border.
    pub panel: Rect,
    /// The panel inside its border: the terminal's background.
    pub inside: Rect,
    /// Where the grid of cells starts. Its size says how many cells fit.
    pub text: Rect,
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
        let panel = window.shrink(self.margin);
        let inside = panel.shrink(self.border);
        let text = inside.shrink(self.padding);
        Layout {
            panel,
            inside,
            text,
        }
    }

    /// The window size that leaves exactly `text_w` x `text_h` pixels for the text.
    pub fn window_size(&self, text_w: usize, text_h: usize) -> (usize, usize) {
        let edge = self.margin + self.border + self.padding;
        (text_w + 2 * edge, text_h + 2 * edge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_and_text_inside_the_window() {
        let layout = Spacing::default().layout(1000, 700);
        // margin 8
        assert_eq!(layout.panel, Rect { x: 8, y: 8, w: 984, h: 684 });
        // + border 1 + padding 6 = 15 from the window's edge
        assert_eq!(layout.text, Rect { x: 15, y: 15, w: 970, h: 670 });
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
