//! What opens over the terminal: the palette, the settings, the sidebar, a
//! card, the search, the ask box.

use super::screens::draw_logo;
use super::shapes::{blend, draw_box, draw_icon, fill_circle, fill_rect, fill_round, pane};
use super::text::{draw_ui_hint, draw_ui_text, keep_end};
use super::{
    ASK_CHARS, Chrome, DOT_RADIUS, ICON_GAP, PALETTE_CHARS, PALETTE_LINES, PALETTE_PADDING,
    RING_WIDTH, SEARCH_CHARS, SETTINGS_CHARS, SIDEBAR_DIM, SWITCHER_CHARS, SWITCHER_LINES, Ui,
};
use crate::ask::Ask;
use crate::card::{self, Card};
use crate::font::Font;
use crate::keys;
use crate::layout::{Layout, Rect};
use crate::palette::Palette;
use crate::search::Search;
use crate::settings::{self, Settings};
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
/// │ ↑↓ go  Shift+↑↓ move  E edit │  its keys, dim
/// └──────────────────────────────┘
/// ```
///
/// A click on a line shows its tab (see `SwitcherPlace::tab_at`).
/// Where the list of tabs is, for `count` tabs with the one at `active` on
/// screen: for drawing it, and for finding the line under a click.
pub(super) struct SwitcherPlace {
    /// The box.
    r: Rect,
    line_h: usize,
    /// Between the box's top and its first line.
    gap: usize,
    /// How many tabs show, from the one at `first`.
    lines: usize,
    first: usize,
}

/// None: no tabs, or a window too narrow for the list.
pub(super) fn switcher_place(
    layout: &Layout,
    ui: &Ui,
    count: usize,
    active: usize,
) -> Option<SwitcherPlace> {
    let area = layout.inside;
    let pad = ui.px(PALETTE_PADDING);
    let b = layout.border;
    let line_h = ui.line_h + 2 * pad;
    let box_w = (SWITCHER_CHARS * ui.advance + 2 * pad).min(area.w.saturating_sub(4 * pad));
    if box_w < 10 * ui.advance || count == 0 {
        return None;
    }
    // The chosen line is as far from the box's top as from its sides.
    let gap = if layout.radius > 0 { ui.px(2) } else { 0 };
    // The lines on screen: the chosen one is always among them.
    let lines = count.min(SWITCHER_LINES);
    let r = Rect {
        x: area.x + (area.w - box_w) / 2,
        y: area.y + 2 * pad,
        w: box_w,
        h: (lines + 1) * line_h + 2 * (b + gap), // and the line of keys
    };
    Some(SwitcherPlace {
        r,
        line_h,
        gap,
        lines,
        first: (active + 1).saturating_sub(lines),
    })
}

impl SwitcherPlace {
    /// The tab whose line is at (x, y): its place among the tabs.
    pub(super) fn tab_at(&self, border: usize, x: usize, y: usize) -> Option<usize> {
        let top = self.r.y + border + self.gap;
        if !self.r.contains(x, y) || y < top {
            return None;
        }
        let row = (y - top) / self.line_h;
        (row < self.lines).then_some(self.first + row)
    }
}

const SWITCHER_KEYS: &str = "↑↓ go   Shift+↑↓ move   E edit";

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
    let Some(place) = switcher_place(layout, ui, tabs.len(), active) else {
        return;
    };
    let SwitcherPlace {
        r,
        line_h,
        gap,
        lines,
        first,
    } = place;
    let pad = ui.px(PALETTE_PADDING);
    let b = layout.border;
    let back = pane(theme, layout);
    let dim = blend(back, theme.foreground, 128);
    draw_box(pixels, width, theme, layout, r, back);
    // The project's color, as on its tab; home: none, the text's gray.
    let project = chrome.project.and_then(|(_, color)| color);
    let accent = theme.accent_for(project);
    let text_x = r.x + b + pad;
    let text_end = r.x + r.w - b - pad;
    for (row, (i, tab)) in tabs.iter().enumerate().skip(first).take(lines).enumerate() {
        let line_y = r.y + b + gap + row * line_h;
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
    // What the keys do here (see `App::switcher_key`).
    let at = (text_x, r.y + b + gap + lines * line_h + pad);
    let max_w = text_end.saturating_sub(text_x);
    draw_ui_hint(pixels, width, ui, SWITCHER_KEYS, at, max_w, dim);
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
    // The last line, chosen, is as far from the box's bottom as from its sides.
    let gap = if layout.radius > 0 { ui.px(2) } else { 0 };
    let box_h = line_h + b + lines * line_h + 2 * b + gap;

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

/// The settings panel (see `settings`), where the palette is: "Settings" and
/// how many are changed, the lines, the keys. The chosen line has a tint of
/// the project's color, as the palette's; a changed value is in the text's
/// color, one as the file has it dim.
///
/// ```text
/// ┌──────────────────────────────────────────┐
/// │ Settings                       1 changed │
/// ├──────────────────────────────────────────┤
/// │ LOOK                                     │
/// │ Glass opacity  1 solid, 0 glass ‹ 0.65 › │
/// │ Grain                               0.10 │
/// ├──────────────────────────────────────────┤
/// │ ↑↓ choose  ←→ change  Enter keep  Esc    │
/// └──────────────────────────────────────────┘
/// ```
pub(super) fn draw_settings(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    (settings, color): (&Settings, Option<u32>),
    font: &Font,
    ui: &mut Ui,
) {
    let area = layout.inside;
    let pad = ui.px(PALETTE_PADDING);
    let b = layout.border;
    // Lines a bit closer than the palette's: there are more.
    let line_h = ui.line_h + pad;
    let head_h = ui.line_h + 2 * pad;
    let box_w = (SETTINGS_CHARS * ui.advance + 2 * pad).min(area.w.saturating_sub(4 * pad));
    if box_w < 24 * ui.advance {
        return; // a window too narrow for it
    }
    // As many lines as fit under the title, over the keys. The chosen one is
    // always among them: with 10 fitting and line 12 chosen, lines 3 to 12.
    let lines = settings.lines();
    let around = 4 * pad + 2 * head_h + pad + 4 * b;
    let fit = (area.h.saturating_sub(around) / line_h).min(lines.len());
    if fit == 0 {
        return;
    }
    let first = (settings.selected() + 1).saturating_sub(fit);

    let x = area.x + (area.w - box_w) / 2;
    let y = area.y + 2 * pad;
    let box_h = b + head_h + b + pad + fit * line_h + b + head_h + b;
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

    // The title, and at its right how many are changed.
    let head_y = y + b + pad;
    let changed = match settings.changes().len() {
        0 => String::new(),
        n => format!("{n} changed"),
    };
    let changed_w = changed.chars().count() * ui.advance;
    let at = (text_x + text_w - changed_w, head_y);
    draw_ui_text(pixels, width, font, ui, &changed, at, changed_w, dim);
    let title_w = text_w.saturating_sub(changed_w + ui.advance);
    let at = (text_x, head_y);
    let title_color = theme.foreground;
    draw_ui_text(
        pixels,
        width,
        font,
        ui,
        "Settings",
        at,
        title_w,
        title_color,
    );
    let sep_y = y + b + head_h;
    fill_rect(pixels, width, x, sep_y, box_w, b, theme.border);

    let list_y = sep_y + b + pad / 2;
    let shown = lines.iter().enumerate().skip(first).take(fit);
    for (row, (i, line)) in shown.enumerate() {
        let line_y = list_y + row * line_h;
        let text_y = line_y + pad / 2;
        let chosen = i == settings.selected();
        if chosen {
            let fill = match color {
                Some(color) => blend(back, color, 60),
                None => theme.selection,
            };
            let inset = if layout.radius > 0 { ui.px(2) } else { 0 };
            let line = Rect {
                x: x + b + inset,
                y: line_y,
                w: box_w - 2 * (b + inset),
                h: line_h,
            };
            fill_round(pixels, width, line, [layout.small_radius(); 4], fill);
        }
        // The name, the chosen one's note, and at the right end a value or a key.
        let (label, note, right, right_color) = match line {
            settings::Line::Title(title) => {
                let at = (text_x, text_y);
                draw_ui_text(pixels, width, font, ui, title, at, text_w, dim);
                continue;
            }
            settings::Line::Setting(setting) => {
                let value = settings::show(setting.kind, &settings.value(setting));
                let value = if chosen {
                    format!("‹ {value} ›")
                } else {
                    value
                };
                let value_color = if settings.is_changed(setting) {
                    theme.foreground
                } else {
                    dim
                };
                let note = match chosen {
                    true => settings.note(setting),
                    false => String::new(),
                };
                (setting.label, note, value, value_color)
            }
            settings::Line::OpenFile => {
                let key = keys::hint(&keys::Action::Settings);
                let note = match chosen {
                    true => "the rest of the settings".to_string(),
                    false => String::new(),
                };
                ("Open config.toml", note, key, dim)
            }
        };
        // A long font's name may take what the name and a gap leave.
        let left_w = (label.chars().count() + 2) * ui.advance;
        let right_w = (right.chars().count() * ui.advance).min(text_w.saturating_sub(left_w));
        let at = (text_x + text_w - right_w, text_y);
        draw_ui_text(pixels, width, font, ui, &right, at, right_w, right_color);
        let room = text_w.saturating_sub(right_w + ui.advance);
        let at = (text_x, text_y);
        let drawn = draw_ui_text(pixels, width, font, ui, label, at, room, theme.foreground);
        if !note.is_empty() {
            let gap = 2 * ui.advance;
            let at = (text_x + drawn + gap, text_y);
            let room = room.saturating_sub(drawn + gap);
            draw_ui_text(pixels, width, font, ui, &note, at, room, dim);
        }
    }

    let sep_y = list_y + fit * line_h + pad / 2;
    fill_rect(pixels, width, x, sep_y, box_w, b, theme.border);
    // "← →" apart: together the font draws them as one arrow, ↔.
    let hint = "↑↓ choose  ← → change  Enter keep  Esc back";
    draw_ui_hint(
        pixels,
        width,
        ui,
        hint,
        (text_x, sep_y + b + pad),
        text_w,
        dim,
    );
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

/// The ask box, over the terminal's bottom, where the answer will go:
///
/// ```text
/// ┌──────────────────────────────────────────────────────────┐
/// │ ? undo the last commit|                          asking… │
/// └──────────────────────────────────────────────────────────┘
/// ```
///
/// At its right end: that the model is asked, or in red why no answer came.
/// Empty, it says what to type, dim.
pub(super) fn draw_ask(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    ask: &Ask,
    font: &Font,
    ui: &mut Ui,
) {
    let area = layout.inside;
    let pad = ui.px(PALETTE_PADDING);
    let b = layout.border;
    let box_w = (ASK_CHARS * ui.advance + 2 * (pad + b)).min(area.w.saturating_sub(4 * pad));
    let box_h = ui.line_h + 2 * (pad + b);
    if box_w < 12 * ui.advance || area.h < box_h + 4 * pad {
        return; // a window too small for it
    }
    let x = area.x + (area.w - box_w) / 2;
    let y = area.y + area.h - box_h - 2 * pad;
    let r = Rect {
        x,
        y,
        w: box_w,
        h: box_h,
    };
    draw_box(pixels, width, theme, layout, r, pane(theme, layout));
    let text_w = box_w - 2 * (b + pad);
    let (text_x, text_y) = (x + b + pad, y + b + pad);
    let dim = blend(theme.frame, theme.foreground, 128);

    // An error can be a sentence: up to two thirds of the box.
    let (status, color) = match ask.error() {
        Some(why) => (why, theme.failed),
        None if ask.waiting() => ("asking…", dim),
        None => ("", dim),
    };
    let status_w = (status.chars().count() * ui.advance).min(text_w * 2 / 3);
    let at = (text_x + text_w - status_w, text_y);
    draw_ui_text(pixels, width, font, ui, status, at, status_w, color);

    // What's typed: its end, when it's longer than the box. Room for the
    // cursor after it.
    let room = text_w.saturating_sub(status_w + ui.advance);
    let at = (text_x, text_y);
    let typed = if ask.query().is_empty() {
        let mark = draw_ui_text(pixels, width, font, ui, "? ", at, room, theme.foreground);
        let hint = "what should the command do?";
        let at = (text_x + mark + ui.advance, text_y);
        let room = room.saturating_sub(mark + ui.advance);
        draw_ui_hint(pixels, width, ui, hint, at, room, dim);
        mark
    } else {
        let chars = (room / ui.advance).saturating_sub(1);
        let query = keep_end(&format!("? {}", ask.query()), chars);
        draw_ui_text(pixels, width, font, ui, &query, at, room, theme.foreground)
    };
    // No cursor while the model is asked: the box takes no typing then.
    if !ask.waiting() {
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
}

/// What a copy says: `copied 124 chars`, `copied 1 char`.
pub(super) fn copied_text(chars: usize) -> String {
    match chars {
        1 => "copied 1 char".to_string(),
        _ => format!("copied {chars} chars"),
    }
}

/// That a selection was copied, and how much of it, in the terminal panel's
/// bottom right corner: a box as wide as its text, for a moment.
///
/// ```text
///                         ┌──────────────────┐
///                         │ copied 124 chars │
///                         └──────────────────┘
/// ```
pub(super) fn draw_copied(
    pixels: &mut [u32],
    width: usize,
    theme: &Theme,
    layout: &Layout,
    chars: usize,
    font: &Font,
    ui: &mut Ui,
) {
    let area = layout.inside;
    let pad = ui.px(PALETTE_PADDING);
    let b = layout.border;
    let text = copied_text(chars);
    let text_w = text.chars().count() * ui.advance;
    let box_w = text_w + 2 * (pad + b);
    let box_h = ui.line_h + 2 * (pad + b);
    if area.w < box_w + 4 * pad || area.h < box_h + 4 * pad {
        return; // a window too small for it
    }
    let x = area.x + area.w - box_w - 2 * pad;
    let y = area.y + area.h - box_h - 2 * pad;
    let r = Rect {
        x,
        y,
        w: box_w,
        h: box_h,
    };
    draw_box(pixels, width, theme, layout, r, pane(theme, layout));
    let color = blend(theme.frame, theme.foreground, 170);
    let at = (x + b + pad, y + b + pad);
    draw_ui_text(pixels, width, font, ui, &text, at, text_w, color);
}

#[cfg(test)]
mod copied_tests {
    use super::copied_text;

    #[test]
    fn one_char_is_not_chars() {
        assert_eq!(copied_text(1), "copied 1 char");
        assert_eq!(copied_text(0), "copied 0 chars");
        assert_eq!(copied_text(124), "copied 124 chars");
    }
}

#[cfg(test)]
mod switcher_tests {
    use super::*;

    #[test]
    fn the_tab_under_a_click() {
        // 20 tabs, the 15th on screen: 12 lines show, from the 4th tab.
        let place = SwitcherPlace {
            r: Rect {
                x: 100,
                y: 50,
                w: 300,
                h: 13 * 30 + 4,
            },
            line_h: 30,
            gap: 2,
            lines: 12,
            first: 3,
        };
        assert_eq!(place.tab_at(0, 150, 52), Some(3));
        assert_eq!(place.tab_at(0, 150, 52 + 30), Some(4));
        assert_eq!(place.tab_at(0, 150, 52 + 12 * 30 - 1), Some(14));
        // The gap above the first line, the line of keys, beside the box: none.
        assert_eq!(place.tab_at(0, 150, 51), None);
        assert_eq!(place.tab_at(0, 150, 52 + 12 * 30), None);
        assert_eq!(place.tab_at(0, 99, 60), None);
    }
}
