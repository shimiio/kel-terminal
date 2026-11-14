//! What takes the whole panel: the start screen (its backdrop, the logo) and
//! the help.

use super::Ui;
use super::overlays::{ProjectRow, draw_project_row};
use super::shapes::{blend, draw_box, fill_circle, fill_rect, fill_round, pane};
use super::text::{draw_ui_hint, draw_ui_text};
use crate::backdrop::Backdrop;
use crate::font::Font;
use crate::help::{Help, Line};
use crate::layout::{Layout, Rect};
use crate::logo;
use crate::start::{self, Start};
use crate::theme::Theme;

/// The start screen (see `start`) over the terminal: Kel's name, the projects
/// used last, the keys.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_start(
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
    // Over the terminal's text, not the panel's round corners.
    let text = layout.text;
    fill_rect(pixels, width, text.x, text.y, text.w, text.h, back);
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
            fill_round(pixels, width, r, [layout.small_radius(); 4], lit);
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
        let palette = crate::keys::hint(&crate::keys::Action::Palette);
        let text = format!("none yet: {palette}, \"Save the tabs as project\"");
        draw_ui_hint(pixels, width, ui, &text, at, col.w, dim);
    }
    let hint_y = start::hint_y(col, layout.scale, projects.len());
    if hint_y + ui.line_h <= layout.text.y + layout.text.h {
        let text = "Type to start   ↑↓ Enter opens a project   Esc";
        draw_ui_hint(pixels, width, ui, text, (left, hint_y), col.w, dim);
    }
    // Where the rest is: all the keys, the settings.
    let help_y = hint_y + ui.line_h;
    if help_y + ui.line_h <= layout.text.y + layout.text.h {
        let help = crate::keys::hint(&crate::keys::Action::Help);
        let text = format!("{help} all the keys and settings");
        draw_ui_hint(pixels, width, ui, &text, (left, help_y), col.w, dim);
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
pub(super) fn draw_help(
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
    let b = layout.border;
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
    draw_box(pixels, width, theme, layout, rect, pane(theme, layout));
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
pub(super) fn draw_logo(
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
