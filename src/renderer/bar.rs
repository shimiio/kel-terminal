//! The tab bar: the project's name, the tabs, the buttons after them, the
//! window's own buttons, and the line that says what went wrong.

use super::shapes::{blend, draw_icon, fill_circle, fill_rect, fill_round, outline};
use super::text::draw_ui_text;
use super::{
    CLOSE_HOVER, Chrome, DOT_RADIUS, ICON_GAP, PROJECT_NAME_CHARS, PROJECT_NAME_LEFT, RING_WIDTH,
    TAB_TEXT_PADDING, TITLE_CHARS, Ui,
};
use crate::font::Font;
use crate::layout::{Bar, Layout, Rect, TitleHit, WindowButton, scaled};
use crate::logo;
use crate::theme::Theme;

/// The tab bar. The tab on screen merges into the panel below it: the panel's
/// color inside, the panel's outline around, and no line between them. The other
/// tabs are only their names, dimmer, on the window's color. Each tab has an
/// icon before its name, and a dot at its right end when it has new output.
pub(super) fn draw_tab_bar(
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
            let r = layout.radius;
            if b > 0 {
                let outer = Rect {
                    h: tab.h + b,
                    ..*tab
                };
                fill_round(pixels, width, outer, [r, r, 0, 0], theme.border);
            }
            // …and the inside, in the panel's color, covering that line: one shape.
            let inner = Rect {
                x: tab.x + b,
                y: tab.y + b,
                w: tab.w.saturating_sub(2 * b),
                h: tab.h,
            };
            let r = r.saturating_sub(b);
            fill_round(pixels, width, inner, [r, r, 0, 0], theme.background);
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
                fill_round(pixels, width, r, [layout.small_radius(); 4], lit);
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

/// The title, where there are no tabs (`tab_bar = "title"`): in the middle of
/// the panel's top, on the panel's own color, `myapp/nvim`: the project in its
/// color (home: `~`), the tab on screen in the text's. With more tabs than one, which one
/// it is (`2/3`), dim, and one mark for what the others have to say: a command
/// failed, or ended well, or new output; else a ring while a command runs here.
pub(super) fn draw_title(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    chrome: &Chrome,
    font: &Font,
    ui: &mut Ui,
) {
    let Chrome { tabs, active, .. } = *chrome;
    let Some(here) = tabs.get(active) else {
        return;
    };
    let dim = blend(theme.background, theme.foreground, 128);
    let accent = theme.accent_for(chrome.project.and_then(|(_, color)| color));
    // A long title (a program's own) is cut: the rest of the line has to fit.
    let name: String = match here.title.chars().count() > TITLE_CHARS {
        true => here
            .title
            .chars()
            .take(TITLE_CHARS - 1)
            .chain(['…'])
            .collect(),
        false => here.title.to_string(),
    };
    let mut parts: Vec<(String, u32)> = Vec::new();
    // Home is `~`, as a shell names it: `~/nvim`, `myapp/nvim`.
    let place = match chrome.project {
        Some((project, _)) => project_name_shown(project),
        None => "~".to_string(),
    };
    parts.push((place, accent));
    parts.push(("/".to_string(), dim));
    parts.push((name, theme.foreground));
    if tabs.len() > 1 {
        parts.push((format!("  {}/{}", active + 1, tabs.len()), dim));
    }
    // The others' news, the worst first; a ring for a command running here.
    let others = || tabs.iter().enumerate().filter(|(i, _)| *i != active);
    let mark = if others().any(|(_, tab)| tab.done == Some(false)) {
        Some((theme.failed, 0.0))
    } else if others().any(|(_, tab)| tab.done == Some(true)) {
        Some((theme.done, 0.0))
    } else if others().any(|(_, tab)| tab.activity) {
        Some((theme.foreground, 0.0))
    } else if here.busy {
        Some((theme.foreground, RING_WIDTH))
    } else {
        None
    };
    let radius = DOT_RADIUS * ui.scale;
    let gap = ui.advance;
    let mark_w = mark.map_or(0, |_| gap + (2.0 * radius).ceil() as usize);

    let bar = layout.tab_bar;
    let chars: usize = parts.iter().map(|(text, _)| text.chars().count()).sum();
    let w = chars * ui.advance + mark_w;
    // In the panel's middle; a line too long for that starts after the
    // sidebar's button.
    let first = layout.sidebar_button.x + layout.sidebar_button.w;
    let middle = layout.panel.x + layout.panel.w / 2;
    let mut x = middle.saturating_sub(w / 2).max(first);
    let end = bar.x + bar.w;
    let y = bar.y + bar.h.saturating_sub(ui.line_h) / 2;
    // Under the mouse: a little lighter behind it, as a button. A click
    // shows the list of tabs.
    if chrome.hovered == Some(TitleHit::Title) {
        let (px, py) = (ui.px(TAB_TEXT_PADDING), ui.px(2));
        let spot = Rect {
            x: x.saturating_sub(px),
            y: y.saturating_sub(py),
            w: (w + 2 * px).min(end.saturating_sub(x.saturating_sub(px))),
            h: ui.line_h + 2 * py,
        };
        let lit = blend(theme.background, theme.foreground, 24);
        fill_round(pixels, width, spot, [layout.small_radius(); 4], lit);
    }
    for (text, color) in &parts {
        let max_w = end.saturating_sub(x);
        draw_ui_text(pixels, width, font, ui, text, (x, y), max_w, *color);
        x += text.chars().count() * ui.advance;
    }
    if let Some((color, ring)) = mark
        && x + mark_w <= end
    {
        let cx = (x + gap) as f32 + radius;
        let cy = bar.y as f32 + bar.h as f32 / 2.0;
        let hole = if ring > 0.0 {
            radius - ring * ui.scale
        } else {
            0.0
        };
        fill_circle(pixels, width, cx, cy, radius, hole, color);
    }
}

/// Minimize, maximize and close, top right. Plain icons in the window's colors;
/// the one under the mouse gets a lighter background, close gets red.
pub(super) fn draw_window_buttons(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    chrome: &Chrome,
) {
    if !layout.window_buttons_show.shown(chrome.mouse_on_bar) {
        return;
    }
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
        // Not lit: nothing behind it. It's on the window's color already, or
        // (under a title) over the panel's corner, which stays as it is.
        if lit_now {
            fill_rect(pixels, width, rect.x, rect.y, rect.w, rect.h, bg);
        }

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
pub(super) fn draw_bar_buttons(
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
    let on_bar = chrome.mouse_on_bar;
    // Not while its sidebar is open: the sidebar is over it, or (with no
    // margin, the sidebar a bit further in) beside it, and says so itself.
    let sidebar_shown = layout.sidebar_shows.shown(on_bar) && chrome.sidebar.is_none();
    // No `+` and `⌄` without tabs.
    let tab_buttons_shown = layout.tab_buttons_show.shown(on_bar) && layout.bar_shows != Bar::Title;
    let buttons = [
        (layout.sidebar_button, TitleHit::Sidebar, sidebar_shown),
        (new_tab, TitleHit::NewTab, tab_buttons_shown),
        (palette, TitleHit::Palette, tab_buttons_shown),
    ];
    for (r, hit, _) in buttons.into_iter().filter(|button| button.2) {
        // A square in the middle of the button's place, 24 x 24 at 100%.
        let side = s(24).min(r.w).min(r.h);
        let (bx, by) = (r.x + (r.w - side) / 2, r.y + (r.h - side) / 2);
        let fg = if chrome.hovered == Some(hit) {
            let spot = Rect {
                x: bx,
                y: by,
                w: side,
                h: side,
            };
            fill_round(pixels, width, spot, [layout.small_radius(); 4], lit);
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
pub(super) fn draw_status(
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
    // The bar's color behind it (the window's; the panel's under a title), so
    // what's under it doesn't show through.
    let color_behind = match layout.bar_shows {
        Bar::Title => theme.background,
        _ => theme.frame,
    };
    let back = x.saturating_sub(ui.px(TAB_TEXT_PADDING));
    fill_rect(pixels, width, back, bar.y, end - back, bar.h, color_behind);
    let y = bar.y + bar.h.saturating_sub(ui.line_h) / 2;
    draw_ui_text(pixels, width, font, ui, &shown, (x, y), text_w, color);
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
pub(super) fn project_name_width(shown: &str, ui: &Ui) -> usize {
    match project_name_pixels(shown, ui) {
        Some((_, w)) => w * project_name_px(ui),
        None => shown.chars().count() * ui.advance,
    }
}

/// The project's name as the tab bar shows it: at most `PROJECT_NAME_CHARS`
/// letters, e.g. "a-very-long-project-name" → "a-very-long-pro…".
pub(super) fn project_name_shown(name: &str) -> String {
    if name.chars().count() <= PROJECT_NAME_CHARS {
        return name.to_string();
    }
    let mut shown: String = name.chars().take(PROJECT_NAME_CHARS - 1).collect();
    shown.push('…');
    shown
}
