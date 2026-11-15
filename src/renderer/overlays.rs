//! What opens over the terminal: the palette, the sidebar, a card, the search.

use super::screens::draw_logo;
use super::shapes::{blend, draw_box, draw_icon, fill_circle, fill_rect, fill_round, pane};
use super::text::{draw_ui_hint, draw_ui_text, keep_end};
use super::{
    Chrome, DOT_RADIUS, ICON_GAP, PALETTE_CHARS, PALETTE_LINES, PALETTE_PADDING, RING_WIDTH,
    SEARCH_CHARS, SIDEBAR_DIM, SWITCHER_CHARS, SWITCHER_LINES, Ui,
};
use crate::card::{self, Card};
use crate::font::Font;
use crate::layout::{Layout, Rect};
use crate::palette::Palette;
use crate::search::Search;
use crate::sidebar::{self, Entry, Live, Row, Sidebar};
use crate::theme::Theme;

/// The list of tabs, while Ctrl is held after Ctrl+Tab, where the palette is:
/// each tab as the tab bar would show it, top to bottom in the tabs' order.
///
/// ```text
/// ┌──────────────────────────────┐
/// │ ▪ nvim                       │
/// │ ▪ claude                   ○ │  the tab on screen: the chosen line
/// │ ▪ server                   ● │  its mark: running, ended, new output
/// └──────────────────────────────┘
/// ```
pub(super) fn draw_switcher(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    chrome: &Chrome,
    font: &Font,
    ui: &mut Ui,
) {
    let Chrome { tabs, active, .. } = *chrome;
    let area = layout.inside;
    let pad = ui.px(PALETTE_PADDING);
    let b = layout.border;
    let line_h = ui.line_h + 2 * pad;
    let box_w = (SWITCHER_CHARS * ui.advance + 2 * pad).min(area.w.saturating_sub(4 * pad));
    if box_w < 10 * ui.advance || tabs.is_empty() {
        return; // a window too narrow for it
    }
    // The lines on screen: the chosen one is always among them.
    let lines = tabs.len().min(SWITCHER_LINES);
    let first = (active + 1).saturating_sub(lines);
    let r = Rect {
        x: area.x + (area.w - box_w) / 2,
        y: area.y + 2 * pad,
        w: box_w,
        h: lines * line_h + 2 * b,
    };
    let back = pane(theme, layout);
    let dim = blend(back, theme.foreground, 128);
    draw_box(pixels, width, theme, layout, r, back);
    // The project's color, as on its tab; home: none, the text's gray.
    let project = chrome.project.and_then(|(_, color)| color);
    let accent = theme.accent_for(project);
    let text_x = r.x + b + pad;
    let text_end = r.x + r.w - b - pad;
    for (row, (i, tab)) in tabs.iter().enumerate().skip(first).take(lines).enumerate() {
        let line_y = r.y + b + row * line_h;
        let on_screen = i == active;
        // The chosen line, as the palette's: a tint of the project's color.
        if on_screen {
            let fill = match project.or(theme.accent) {
                Some(color) => blend(back, color, 60),
                None => theme.selection,
            };
            let inset = if layout.radius > 0 { ui.px(2) } else { 0 };
            let line = Rect {
                x: r.x + b + inset,
                y: line_y,
                w: r.w - 2 * (b + inset),
                h: line_h,
            };
            fill_round(pixels, width, line, [layout.small_radius(); 4], fill);
        }
        let color = if on_screen { theme.foreground } else { dim };
        let icon = ui.icons.get(tab.icon);
        let (icon_w, icon_h) = (icon.w * ui.icon_scale, icon.h * ui.icon_scale);
        let icon_y = line_y + line_h.saturating_sub(icon_h) / 2;
        let tint = if on_screen { accent } else { dim };
        draw_icon(pixels, width, icon, ui.icon_scale, tint, text_x, icon_y);
        // The mark at the right end, as on a tab: a running command's ring,
        // then how one ended, then plain new output.
        let mark = match (tab.busy, tab.done) {
            (true, _) => Some((theme.foreground, RING_WIDTH)),
            (false, Some(true)) => Some((theme.done, 0.0)),
            (false, Some(false)) => Some((theme.failed, 0.0)),
            (false, None) if tab.activity => Some((theme.foreground, 0.0)),
            _ => None,
        };
        let radius = DOT_RADIUS * ui.scale;
        if let Some((mark_color, ring)) = mark {
            let cx = text_end as f32 - radius;
            let cy = line_y as f32 + line_h as f32 / 2.0;
            let hole = if ring > 0.0 {
                radius - ring * ui.scale
            } else {
                0.0
            };
            fill_circle(pixels, width, cx, cy, radius, hole, mark_color);
        }
        let x = text_x + icon_w + ui.px(ICON_GAP);
        let max_w = text_end.saturating_sub(x + (4.0 * radius) as usize);
        draw_ui_text(
            pixels,
            width,
            font,
            ui,
            tab.title,
            (x, line_y + pad),
            max_w,
            color,
        );
    }
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
pub(super) fn draw_palette(
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
    let b = layout.border;
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

    let back = pane(theme, layout);
    let dim = blend(back, theme.foreground, 128);
    let r = Rect {
        x,
        y,
        w: box_w,
        h: box_h,
    };
    draw_box(pixels, width, theme, layout, r, back);
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
                Some(color) => blend(back, color, 60),
                None => theme.selection,
            };
            // With round corners, clear of the box's own.
            let inset = if layout.radius > 0 { ui.px(2) } else { 0 };
            let line = Rect {
                x: x + b + inset,
                y: line_y,
                w: box_w - 2 * (b + inset),
                h: line_h,
            };
            fill_round(pixels, width, line, [layout.small_radius(); 4], fill);
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
pub(super) fn draw_sidebar(
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
    let b = layout.border;
    let back = blend(theme.frame, theme.foreground, 8);
    draw_box(pixels, width, theme, layout, panel, back);
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
            fill_round(pixels, width, r, [layout.small_radius(); 4], lit);
        }
        let text_y = r.y + r.h.saturating_sub(ui.line_h) / 2;
        let look = match sidebar.row(row) {
            Row::Save => {
                // After a line across.
                let line_y = r.y - ui.px(8);
                let line = theme.border;
                fill_rect(pixels, width, panel.x + pad, line_y, text_w, b, line);
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
pub(super) fn draw_card(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    card: &Card,
    rect: Rect,
    font: &Font,
    ui: &mut Ui,
) {
    let back = pane(theme, layout);
    draw_box(pixels, width, theme, layout, rect, back);
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
        let field_box = Rect {
            x: bx,
            y: by,
            w: bw,
            h: bh,
        };
        let color = blend(back, theme.foreground, lit);
        fill_round(pixels, width, field_box, [layout.small_radius(); 4], color);
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
        // A choice has no cursor: it isn't typed.
        if focused && field.choices.is_empty() {
            let cx = tx + cursor * ui.advance;
            fill_rect(pixels, width, cx, text_y, ui.px(2), ui.line_h, theme.cursor);
        }
    }
    // Under the fields: what the one with the cursor does, and the keys. Or,
    // in red, why what's typed can't be kept.
    let n = card.fields().len();
    let note = card.note();
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
pub(super) struct ProjectRow<'a> {
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
    pub(super) fn of(entry: &'a Entry) -> ProjectRow<'a> {
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
pub(super) fn draw_project_row(
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

/// The search box, in the terminal panel's top right corner:
///
/// ```text
/// ┌────────────────────────────┐
/// │ > error|          3 of 17  │  what's typed, a cursor, which match
/// └────────────────────────────┘
/// ```
pub(super) fn draw_search(
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
    let b = layout.border;
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
    draw_box(pixels, width, theme, layout, r, pane(theme, layout));
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
