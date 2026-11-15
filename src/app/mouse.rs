//! The mouse: moves, buttons, the wheel. Selecting text, links, the title
//! bar's buttons and drags, the sidebar's peek, and what goes to a program
//! that asked for the mouse.

use super::{
    App, BAR_TIME, By, DOUBLE_CLICK, PEEK_DELAY, Selecting, Selection, SidebarDrag, TAB_DRAG_START,
    TabDrag,
};
use crate::card::Target;
use crate::keys::{self, Action};
use crate::layout::{Bar, TitleHit, WindowButton};
use crate::mouse::{self, Tracking};
use crate::sidebar::Row;
use crate::{links, open};
use std::time::Instant;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::window::{CursorIcon, Window};

/// Two cells of the view (a word's or a line's ends) by lasting rows: `top` is
/// the lasting number of the view's first row (see `Grid::first_visible_row`).
fn lasting(
    top: usize,
    (start, end): ((usize, usize), (usize, usize)),
) -> ((usize, usize), (usize, usize)) {
    ((top + start.0, start.1), (top + end.0, end.1))
}

impl App {
    /// Mouse wheel or touchpad: scroll the view into the scrollback.
    pub(super) fn on_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        // The help scrolls instead: 3 lines a notch.
        if let Some(help) = &mut self.help {
            if let MouseScrollDelta::LineDelta(_, y) = delta {
                help.scroll_by((-y * 3.0) as isize);
                self.request_redraw();
            }
            return;
        }
        // Over the sidebar: its list, a project a notch. The row now under the
        // mouse is the chosen one, as if the mouse had moved onto it.
        let (x, y) = self.mouse_pos;
        let panel = self.layout.sidebar();
        if let Some(sidebar) = &mut self.sidebar
            && panel.contains(x, y)
        {
            let step = match delta {
                MouseScrollDelta::LineDelta(_, notches) => -notches.round() as isize,
                MouseScrollDelta::PixelDelta(pos) => {
                    // A touchpad: a project's height of moving is one.
                    let row_h = crate::layout::scaled(32, self.layout.scale) as f32;
                    self.scroll_rest -= pos.y as f32 / row_h;
                    let whole = self.scroll_rest.trunc();
                    self.scroll_rest -= whole;
                    whole as isize
                }
            };
            if sidebar.scroll_by(step) {
                if let Some(row) = sidebar.row_at(panel, self.layout.scale, x, y) {
                    sidebar.select(row);
                }
                self.request_redraw();
            }
            return;
        }
        if self.start.is_some() {
            return; // nothing to scroll under it
        }
        let Some(renderer) = &self.renderer else {
            return;
        };
        let (_, cell_h) = renderer.cell_size();
        // How many lines this event is worth. Often not a whole number:
        // a touchpad sends many small steps, like 0.1 of a wheel notch.
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => y * 3.0, // 3 lines per wheel notch
            MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / cell_h as f32,
        };
        if self.program_takes_mouse() {
            // The program scrolls by itself (nvim: 3 lines a notch): it gets notches.
            self.scroll_rest += lines / 3.0;
            let notches = self.scroll_rest.trunc();
            self.scroll_rest -= notches;
            let button = if notches > 0.0 {
                mouse::Button::WheelUp
            } else {
                mouse::Button::WheelDown
            };
            for _ in 0..notches.abs() as usize {
                self.report_mouse(mouse::Event::Press(button));
            }
            return;
        }
        // Add it to the leftover from before. Scroll by the whole lines, keep the rest.
        self.scroll_rest += lines;
        let whole = self.scroll_rest.trunc();
        self.scroll_rest -= whole;
        if whole != 0.0 {
            self.tab()
                .terminal
                .lock()
                .unwrap()
                .grid
                .scroll_view(whole as i32);
            // Scrolled with the button held: the selection grows to the cell
            // now under the mouse.
            self.extend_selection();
            self.request_redraw();
        }
    }

    /// The mouse moved: remember which cell it's over, and extend the selection while dragging.
    pub(super) fn on_mouse_move(&mut self, position: PhysicalPosition<f64>) {
        let Some(renderer) = &self.renderer else {
            return;
        };
        let (cell_w, cell_h) = renderer.cell_size();
        // Pixels to cells, counted from where the text starts. e.g. the text at
        // (15, 15), the mouse at (100, 60), 11x24 cells: (85, 45) → col 7, row 1.
        // Left of or above the text, `saturating_sub` gives 0: the first col/row.
        let (x, y) = (position.x as usize, position.y as usize);
        let text = self.layout.text;
        let col = x.saturating_sub(text.x) / cell_w;
        let row = y.saturating_sub(text.y) / cell_h;
        let moved = (row, col) != self.mouse_cell;
        self.mouse_pos = (x, y);
        self.mouse_inside = true;
        self.mouse_cell = (row, col);
        // At the window's edge: resize arrows, and no lit button under them. A
        // maximized window can't be resized, so it has no edges.
        let maximized = self.window.as_ref().is_some_and(|w| w.is_maximized());
        let edge = if maximized {
            None
        } else {
            self.layout.resize_edge(x, y)
        };
        if edge != self.resize_edge {
            self.resize_edge = edge;
            if let Some(window) = &self.window {
                window.set_cursor(edge.map_or(CursorIcon::Default, CursorIcon::from));
            }
        }
        self.update_hovered();
        self.leave_peek();
        // The sidebar is open: the row under the mouse is chosen, nothing else.
        // A project held: it follows the mouse, a place at a time.
        if let Some(sidebar) = &mut self.sidebar {
            let (panel, scale) = (self.layout.sidebar(), self.layout.scale);
            if let Some(drag) = &mut self.sidebar_drag {
                let start = crate::layout::scaled(TAB_DRAG_START, scale);
                if drag.moving || y.abs_diff(drag.from_y) >= start {
                    drag.moving = true;
                    let from = sidebar.selected().saturating_sub(1);
                    let to = sidebar.project_slot(panel, scale, y);
                    if sidebar.move_project(from, to) {
                        self.request_redraw();
                    }
                }
                return;
            }
            let row = sidebar.row_at(panel, scale, x, y);
            if let Some(row) = row.filter(|&row| row != sidebar.selected()) {
                sidebar.select(row);
                self.request_redraw();
            }
            return;
        }
        // The start screen: the row under the mouse is lit.
        if let Some(start) = &mut self.start {
            let row = start.row_at(self.layout.text, self.layout.scale, x, y);
            if start.hover(row) {
                self.request_redraw();
            }
            return;
        }
        if moved {
            self.update_link();
        }
        if self.tab_drag.is_some() {
            self.drag_tab();
            return;
        }

        if self.selecting.is_some() {
            self.extend_selection();
        } else if moved
            && (self.mouse_button.is_some() || self.over_text() && self.program_takes_mouse())
        {
            // Dragging in the program, or just moving if it asked for every move.
            self.report_mouse(mouse::Event::Move(self.mouse_button));
        }
    }

    /// The left button is held in the terminal: the selection goes on to the
    /// cell under the mouse. After the mouse moved, or the view scrolled under it.
    fn extend_selection(&mut self) {
        let Some(Selecting { from, by }) = self.selecting else {
            return;
        };
        let (row, col) = self.mouse_cell;
        let selection = {
            let grid = &self.tab().terminal.lock().unwrap().grid;
            let top = grid.first_visible_row();
            match by {
                // Off the pressed cell: the selection starts. Back on it later:
                // it stays, one cell, chosen on purpose.
                By::Cell => match self.selection {
                    None if (top + row, col) == from => None,
                    _ => Some(Selection {
                        start: from,
                        end: (top + row, col),
                    }),
                },
                // Word by word, line by line.
                By::Word(clicked) => Some(clicked.joined(lasting(top, grid.word_at(row, col)))),
                By::Line(clicked) => Some(clicked.joined(lasting(top, grid.line_at(row)))),
            }
        };
        if selection != self.selection {
            self.selection = selection;
            self.request_redraw();
        }
    }

    /// Finds the link under the mouse again, while Ctrl (Cmd on a Mac) is held:
    /// it's underlined and the mouse becomes a hand. After the mouse moved, or
    /// Ctrl changed.
    pub(super) fn update_link(&mut self) {
        let look = keys::command_key(self.modifiers)
            && self.sidebar.is_none()
            && self.start.is_none()
            && self.mouse_inside
            && self.over_text()
            && self.selecting.is_none();
        let link = if look {
            let (row, col) = self.mouse_cell;
            self.tab().terminal.lock().unwrap().grid.link_at(row, col)
        } else {
            None
        };
        if link == self.link {
            return;
        }
        if link.is_some() != self.link.is_some()
            && let Some(window) = &self.window
        {
            // Off a link: back to what the mouse was (resize arrows at an edge).
            let off = self
                .resize_edge
                .map_or(CursorIcon::Default, CursorIcon::from);
            window.set_cursor(if link.is_some() {
                CursorIcon::Pointer
            } else {
                off
            });
        }
        self.link = link;
        self.request_redraw();
    }

    /// Opens a link (see `links::target`): a web page in the default browser,
    /// as clicking it in another program would; a file shown in the file
    /// manager, its folder open and the file chosen (a folder: opened). Nothing
    /// is run (see `open`).
    fn open_link(&mut self, url: &str) {
        let opened = match links::target(url) {
            Some(links::Target::Web) => open::page(url),
            Some(links::Target::File(path)) if path.is_dir() => open::folder(&path),
            Some(links::Target::File(path)) if path.exists() => open::reveal(&path),
            Some(links::Target::File(path)) => {
                self.set_status(Some(format!("no such file: {}", path.display())));
                return;
            }
            None => {
                self.set_status(Some(format!("Kel opens web and file links only: {url}")));
                return;
            }
        };
        if let Err(e) = opened {
            self.set_status(Some(format!("can't open {url}: {e}")));
        }
    }

    /// The program on screen asked for the mouse (nvim does), and Shift isn't held:
    /// Shift + mouse is always Kel's, to select text the program shows.
    fn program_takes_mouse(&self) -> bool {
        !self.modifiers.shift_key() && self.tab().terminal.lock().unwrap().mouse != Tracking::Off
    }

    /// The mouse is over the terminal panel: not the title bar, not an edge.
    fn over_text(&self) -> bool {
        self.resize_edge.is_none() && !self.over_bar()
    }

    /// The mouse's place is in the title bar, and the bar is there (see `bar_up`).
    fn over_bar(&self) -> bool {
        let (x, y) = self.mouse_pos;
        self.bar_up() && self.layout.title_bar.contains(x, y)
    }

    /// The tab bar shows. Always, unless the config hides it (`tab_bar` in
    /// `[layout]`). Hidden, it comes over the panel's top:
    /// - while Kel says something there: a problem, a question, why a key did
    ///   nothing; and while a tab's card hangs from it;
    /// - `"hover"` only: called by the mouse (see `update_hovered`), while a
    ///   tab is dragged, and for `BAR_TIME` by itself (see `show_bar`).
    pub(super) fn bar_up(&self) -> bool {
        let mode = self.config.layout.tab_bar;
        if matches!(mode, Bar::Always | Bar::Title) {
            return true;
        }
        let says = self.status.is_some() || self.pending_close.is_some() || self.notice.is_some();
        let card = matches!(&self.card, Some(card) if matches!(card.target, Target::Tab(_)));
        let wanted = self.bar_peek || self.tab_drag.is_some() || self.bar_until.is_some();
        says || card || mode == Bar::Hover && wanted
    }

    /// A hidden tab bar (`tab_bar = "hover"`) shows for `BAR_TIME`: the tab on
    /// screen changed, or a command ended in another one. Where you are, and
    /// what's new, then the plain terminal again.
    pub(super) fn show_bar(&mut self) {
        if self.config.layout.tab_bar == Bar::Hover {
            self.bar_until = Some(Instant::now() + BAR_TIME);
            self.request_redraw();
        }
    }

    /// Tells the program on screen about `event` at the mouse's cell, the way it asked.
    fn report_mouse(&mut self, event: mouse::Event) {
        let bytes = {
            let terminal = self.tab().terminal.lock().unwrap();
            // In the space around the text: the nearest cell.
            let row = self.mouse_cell.0.min(terminal.grid.rows() - 1);
            let col = self.mouse_cell.1.min(terminal.grid.cols() - 1);
            let (tracking, sgr) = (terminal.mouse, terminal.mouse_sgr);
            mouse::report(tracking, sgr, event, self.modifiers, (row, col))
        };
        if let Some(bytes) = bytes {
            self.tab_mut().write(&bytes);
        }
    }

    /// A mouse button: the program's, if it asked for the mouse; else Kel's (the
    /// left one selects text). The title bar and the edges are always Kel's.
    pub(super) fn on_mouse_button(&mut self, state: ElementState, button: MouseButton) {
        if state == ElementState::Pressed {
            self.sidebar_peek = false; // a click in a peek: it stays
        }
        // A card: a click on a field puts the cursor there; one outside keeps
        // what's typed and closes it (as renaming a file in Explorer does).
        if self.card.is_some() && state == ElementState::Pressed {
            let (x, y) = self.mouse_pos;
            let rect = self.card_rect().filter(|rect| rect.contains(x, y));
            match (rect, &mut self.card) {
                (Some(rect), Some(card)) => {
                    if let Some(i) = card.field_at(rect, self.layout.scale, x, y) {
                        card.focus_field(i);
                    }
                }
                _ => self.apply_card(),
            }
            self.request_redraw();
            return;
        }
        // A click anywhere closes the help, or the palette, and does nothing else.
        if self.help.is_some() && state == ElementState::Pressed {
            self.help = None;
            self.request_redraw();
            return;
        }
        if self.palette.is_some() && state == ElementState::Pressed {
            self.palette = None;
            self.request_redraw();
            return;
        }
        // A project held in the sidebar, let go: moved, its new place is kept;
        // not moved, it was a click: it opens.
        if state == ElementState::Released
            && button == MouseButton::Left
            && let Some(drag) = self.sidebar_drag.take()
        {
            if drag.moving {
                self.save_sidebar_order();
            } else {
                self.sidebar_choose();
            }
            self.request_redraw();
            return;
        }
        // The sidebar: a click on a row does it, one outside the panel closes it.
        if let Some(sidebar) = &mut self.sidebar
            && state == ElementState::Pressed
        {
            let (x, y) = self.mouse_pos;
            let panel = self.layout.sidebar();
            if !panel.contains(x, y) {
                self.sidebar = None;
                self.request_redraw();
            } else if let Some(row) = sidebar.row_at(panel, self.layout.scale, x, y) {
                sidebar.select(row);
                let project = matches!(sidebar.row(row), Row::Project(_));
                match button {
                    // A project: held, it can be dragged; let go, it opens.
                    MouseButton::Left if project => {
                        self.sidebar_drag = Some(SidebarDrag {
                            from_y: y,
                            moving: false,
                        });
                    }
                    MouseButton::Left => self.sidebar_choose(),
                    // A right click: its card (a project's).
                    MouseButton::Right => self.sidebar_edit(),
                    _ => {}
                }
                self.request_redraw();
            }
            return;
        }
        // The start screen: a click on a project opens it, one elsewhere over
        // the terminal puts the screen away.
        if let Some(start) = &self.start
            && state == ElementState::Pressed
            && self
                .layout
                .inside
                .contains(self.mouse_pos.0, self.mouse_pos.1)
        {
            let (x, y) = self.mouse_pos;
            let row = start.row_at(self.layout.text, self.layout.scale, x, y);
            match row.and_then(|row| start.open(row)) {
                Some(action) if button == MouseButton::Left => self.run(action),
                _ => self.start = None,
            }
            self.request_redraw();
            return;
        }
        // A right click on a tab: its card.
        if button == MouseButton::Right && state == ElementState::Pressed {
            let (x, y) = self.mouse_pos;
            let hit = self.layout.title_hit(x, y, self.tabs.len());
            let hit = hit.filter(|_| self.over_bar()); // not a bar that's hidden
            if let Some(TitleHit::Tab { index, .. }) = hit {
                self.show_tab(index);
                self.open_tab_card(index);
                return;
            }
            // On the title: the card of the tab on screen.
            if hit == Some(TitleHit::Title) {
                self.open_tab_card(self.active);
                return;
            }
        }
        // A middle click on a tab closes it, as in a browser.
        if button == MouseButton::Middle && state == ElementState::Pressed {
            let (x, y) = self.mouse_pos;
            let hit = self.layout.title_hit(x, y, self.tabs.len());
            let hit = hit.filter(|_| self.over_bar()); // not a bar that's hidden
            if let Some(TitleHit::Tab { index, .. }) = hit {
                self.close_tab_at(index, "middle click");
                return;
            }
        }
        // Ctrl+click (Cmd+click on a Mac) on a link opens it, even in a program
        // that has the mouse. Elsewhere Ctrl+click is what it was.
        if button == MouseButton::Left
            && state == ElementState::Pressed
            && keys::command_key(self.modifiers)
            && let Some(link) = &self.link
        {
            let url = link.url.clone();
            self.open_link(&url);
            return;
        }
        let button = match button {
            MouseButton::Left => mouse::Button::Left,
            MouseButton::Middle => mouse::Button::Middle,
            MouseButton::Right => mouse::Button::Right,
            _ => return,
        };
        match state {
            // The program got the press, so it gets the release, wherever the
            // mouse is now and whatever is held.
            ElementState::Released if self.mouse_button == Some(button) => {
                self.mouse_button = None;
                self.report_mouse(mouse::Event::Release(button));
            }
            ElementState::Pressed if self.over_text() && self.program_takes_mouse() => {
                self.mouse_button = Some(button);
                self.report_mouse(mouse::Event::Press(button));
            }
            _ if button == mouse::Button::Left => self.on_left_button(state),
            _ => {}
        }
    }

    /// Finds what's under the mouse in the title bar again: after the mouse moved,
    /// or after the things there moved (a new tab pushes the + to the right).
    /// Nothing at the window's edge: the resize arrows are there.
    pub(super) fn update_hovered(&mut self) {
        let (x, y) = self.mouse_pos;
        // A hidden tab bar (`tab_bar = "hover"`): the mouse above the text
        // calls it, and it stays while the mouse is on it. Not with a button
        // held: a selection dragged to the top is still a selection.
        let held = self.selecting.is_some() || self.mouse_button.is_some();
        let peek = self.config.layout.tab_bar == Bar::Hover
            && self.mouse_inside
            && !held
            && match self.bar_up() {
                true => self.layout.title_bar.contains(x, y),
                false => self.layout.calls_bar(y),
            };
        if peek != self.bar_peek {
            self.bar_peek = peek;
            self.request_redraw();
        }
        let on_bar = self.mouse_inside && self.over_bar();
        if on_bar != self.mouse_on_bar {
            self.mouse_on_bar = on_bar;
            self.request_redraw();
        }
        let hit = if !on_bar || self.resize_edge.is_some() {
            None
        } else {
            self.layout.title_hit(x, y, self.tabs.len())
        };
        self.set_hovered(hit);
    }

    /// What's under the mouse in the title bar changed: draw it again. Onto
    /// the sidebar button, with nothing open: the wait for a peek starts (see `peek`).
    fn set_hovered(&mut self, hit: Option<TitleHit>) {
        if self.hovered != hit {
            self.hovered = hit;
            self.request_redraw();
            let nothing_open = self.sidebar.is_none()
                && self.palette.is_none()
                && self.card.is_none()
                && self.help.is_none();
            self.burger_since = (hit == Some(TitleHit::Sidebar) && nothing_open).then(Instant::now);
        }
    }

    /// The mouse has rested on the sidebar button for `PEEK_DELAY`: the sidebar opens, as a
    /// peek, closing again when the mouse leaves it (see `leave_peek`).
    pub(super) fn peek(&mut self) {
        let due = self
            .burger_since
            .is_some_and(|since| Instant::now() >= since + PEEK_DELAY);
        if !due {
            return;
        }
        self.burger_since = None;
        if self.sidebar.is_none() {
            self.run(Action::Sidebar);
            self.sidebar_peek = true;
        }
    }

    /// A peek closes when the mouse is out of the sidebar (or the window), unless
    /// a click or a key kept it.
    pub(super) fn leave_peek(&mut self) {
        let (x, y) = self.mouse_pos;
        let inside = self.mouse_inside && self.layout.sidebar().contains(x, y);
        if self.sidebar_peek && self.sidebar.is_some() && !inside {
            self.sidebar = None;
            self.sidebar_peek = false;
            self.request_redraw();
        }
    }

    /// A click in the title bar: a button, a tab or its ×, or the empty bar, which
    /// moves the window (and maximizes it on a double click), like a Windows title bar.
    fn on_title_bar_click(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let (x, y) = self.mouse_pos;
        match self.layout.title_hit(x, y, self.tabs.len()) {
            Some(TitleHit::Window(button)) => match button {
                WindowButton::Minimize => window.set_minimized(true),
                WindowButton::Maximize => window.set_maximized(!window.is_maximized()),
                WindowButton::Close => self.close_window(),
            },
            Some(TitleHit::NewTab) => self.new_tab(),
            Some(TitleHit::Sidebar) => self.run(Action::Sidebar),
            Some(TitleHit::Palette) => self.run(Action::Palette),
            Some(TitleHit::Tab { index, close: true }) => self.close_tab_at(index, "×"),
            Some(TitleHit::Tab {
                index,
                close: false,
            }) => {
                // On screen right away; held, it can be dragged to another place.
                // Clicked twice: its card.
                let id = self.tabs[index].id;
                let now = Instant::now();
                let double = self
                    .last_tab_click
                    .is_some_and(|(last, at)| last == id && now - at < DOUBLE_CLICK);
                self.show_tab(index);
                if double {
                    self.last_tab_click = None;
                    self.open_tab_card(index);
                    return;
                }
                self.last_tab_click = Some((id, now));
                self.tab_drag = Some(TabDrag {
                    from_x: x,
                    moving: false,
                });
            }
            // The title moves the window too.
            Some(TitleHit::Title) | None => self.on_empty_bar_click(&window),
        }
    }

    /// The empty part of the title bar: drag to move the window, double-click to
    /// maximize or restore it.
    fn on_empty_bar_click(&mut self, window: &Window) {
        let now = Instant::now();
        let double = self
            .last_click
            .is_some_and(|last| now - last < DOUBLE_CLICK);
        if double {
            window.set_maximized(!window.is_maximized());
            self.last_click = None;
        } else {
            self.last_click = Some(now);
            // Windows moves the window with the mouse until the button is released.
            let _ = window.drag_window();
        }
    }

    /// Left mouse button: pressing and moving starts a new selection, releasing
    /// finishes it. A click alone selects nothing, not even its cell.
    fn on_left_button(&mut self, state: ElementState) {
        match state {
            // At the window's edge: Windows resizes the window with the mouse
            // until the button is released.
            ElementState::Pressed if self.resize_edge.is_some() => {
                if let (Some(window), Some(edge)) = (&self.window, self.resize_edge) {
                    let _ = window.drag_resize_window(edge);
                }
            }
            // Above the panel: Kel's title bar. There's no text to select there.
            ElementState::Pressed if self.over_bar() => self.on_title_bar_click(),
            // The old selection goes; a new one starts when the mouse moves on.
            // Clicked again, on the same cell: the word, then the line.
            ElementState::Pressed => {
                let cell = self.mouse_cell;
                let now = Instant::now();
                let clicks = match self.text_click {
                    Some((at, on, n)) if on == cell && now - at < DOUBLE_CLICK => n % 3 + 1,
                    _ => 1,
                };
                self.text_click = Some((now, cell, clicks));
                // The selection is kept by lasting rows: the view's top row's
                // number is added to the view's rows.
                let (top, by) = {
                    let grid = &self.tab().terminal.lock().unwrap().grid;
                    let top = grid.first_visible_row();
                    let by = match clicks {
                        1 => By::Cell,
                        _ => {
                            let (start, end) = lasting(
                                top,
                                match clicks {
                                    2 => grid.word_at(cell.0, cell.1),
                                    _ => grid.line_at(cell.0),
                                },
                            );
                            let clicked = Selection { start, end };
                            match clicks {
                                2 => By::Word(clicked),
                                _ => By::Line(clicked),
                            }
                        }
                    };
                    (top, by)
                };
                self.selection = match by {
                    By::Cell => None,
                    By::Word(clicked) | By::Line(clicked) => Some(clicked),
                };
                let from = (top + cell.0, cell.1);
                self.selecting = Some(Selecting { from, by });
                self.request_redraw();
            }
            // A selection made with this press: copied now, with `copy_on_select`.
            ElementState::Released => {
                self.tab_drag = None;
                let selected = self.selecting.take().is_some() && self.selection.is_some();
                if selected && self.config.copy_on_select {
                    self.copy_selection();
                }
            }
        }
    }
}
