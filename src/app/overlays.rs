//! What opens over the terminal and takes the keys: the sidebar, the cards,
//! the search, the palette, the start screen, the help.

use super::{App, actions::clipboard_text};
use crate::card::{self, Card, Target};
use crate::keys::{self, Action};
use crate::layout::Rect;
use crate::project;
use crate::sidebar::{Row, Sidebar};
use winit::event::KeyEvent;
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

impl App {
    /// A key while the sidebar is open: Up/Down choose a row, Enter does it, Esc
    /// closes. Everything else waits until it's closed.
    pub(super) fn sidebar_key(&mut self, event: &KeyEvent) {
        let alt = self.modifiers.alt_key();
        let shift = self.modifiers.shift_key();
        let Some(sidebar) = &mut self.sidebar else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.sidebar = None,
            Key::Named(NamedKey::ArrowUp) if alt => self.sidebar_shift(-1),
            Key::Named(NamedKey::ArrowDown) if alt => self.sidebar_shift(1),
            Key::Named(NamedKey::ArrowUp) => sidebar.move_selection(-1),
            Key::Named(NamedKey::ArrowDown) => sidebar.move_selection(1),
            Key::Named(NamedKey::Enter) => self.sidebar_choose(),
            Key::Named(NamedKey::Delete) if shift => self.sidebar_delete(),
            Key::Named(NamedKey::Delete) => self.sidebar_close(),
            Key::Named(NamedKey::F2) => self.sidebar_edit(),
            _ => {}
        }
        self.request_redraw();
    }

    /// Opens the card of the tab at `index`: its name, and in a project its
    /// command (see `card`).
    pub(super) fn open_tab_card(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let name = tab.name.as_deref();
        let command = tab.start_command.as_deref();
        let in_project = self.project.is_some();
        self.card = Some(Card::tab(tab.id, name, command, tab.start, in_project));
        self.palette = None;
        self.search = None;
        self.sidebar = None;
        self.request_redraw();
    }

    /// Where the open card is: under its tab.
    pub(super) fn card_rect(&self) -> Option<Rect> {
        let card = self.card.as_ref()?;
        let (window, scale) = (self.layout.window, self.layout.scale);
        let anchor = match &card.target {
            Target::Tab(id) => {
                let index = self.tabs.iter().position(|tab| tab.id == *id)?;
                self.layout.tabs(self.tabs.len())[index]
            }
            // Under its row in the sidebar (row 0 is home).
            Target::Project(name) => {
                let sidebar = self.sidebar.as_ref()?;
                let projects = sidebar.projects();
                let index = projects.iter().position(|p| p.name == *name)?;
                sidebar.row_rect(self.layout.sidebar(), scale, index + 1)?
            }
        };
        Some(card::rect(anchor, window, scale, card.fields().len()))
    }

    /// A key while a card is open: it's typed in the field with the cursor. Tab
    /// or ↑↓ go to another field, Enter keeps what's typed, Esc doesn't.
    pub(super) fn card_key(&mut self, event: &KeyEvent) {
        // Kel's key (Cmd+V on a Mac), and the one for a word: Option on a Mac.
        let ctrl = keys::command_key(self.modifiers);
        let word = if keys::MAC {
            self.modifiers.alt_key()
        } else {
            self.modifiers.control_key()
        };
        let typed = !self.modifiers.control_key() && !self.modifiers.super_key();
        let shift = self.modifiers.shift_key();
        let Some(card) = &mut self.card else {
            return;
        };
        card.error = None;
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.card = None,
            Key::Named(NamedKey::ArrowLeft) if card.step(-1) => {}
            Key::Named(NamedKey::ArrowRight) if card.step(1) => {}
            Key::Named(NamedKey::Enter) => self.apply_card(),
            Key::Named(NamedKey::Tab) => card.next_field(if shift { -1 } else { 1 }),
            Key::Named(NamedKey::ArrowUp) => card.next_field(-1),
            Key::Named(NamedKey::ArrowDown) => card.next_field(1),
            Key::Named(NamedKey::ArrowLeft) => card.focused().move_cursor(-1),
            Key::Named(NamedKey::ArrowRight) => card.focused().move_cursor(1),
            // A choice isn't typed: ←→ change it.
            Key::Named(NamedKey::Backspace | NamedKey::Delete) if card.on_choice() => {}
            Key::Named(NamedKey::Home) => card.focused().jump(false),
            Key::Named(NamedKey::End) => card.focused().jump(true),
            Key::Named(NamedKey::Backspace) if word => card.focused().backspace_word(),
            Key::Named(NamedKey::Backspace) => card.focused().backspace(),
            Key::Named(NamedKey::Delete) => card.focused().delete(),
            // Ctrl+V (or Kel's Ctrl+Shift+V): the clipboard, as one line.
            _ if ctrl && event.physical_key == PhysicalKey::Code(KeyCode::KeyV) => {
                if let Some(text) = clipboard_text() {
                    card.type_text(&text);
                }
            }
            // Typed letters, not Ctrl+letter (see `palette_key`).
            _ if typed => {
                if let Some(text) = event.text.as_deref() {
                    card.type_text(text);
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    /// Enter, or a click outside the card: what's typed is kept. A tab's name and
    /// command go into the project's file at once. The command starts the next
    /// time the project opens, or with Ctrl+Shift+R: not now.
    pub(super) fn apply_card(&mut self) {
        let Some(mut card) = self.card.take() else {
            return;
        };
        let id = match &card.target {
            Target::Tab(id) => *id,
            Target::Project(old) => {
                let old = old.clone();
                let new = card.value("Name").unwrap_or_default();
                let changed = card.color().and_then(|color| {
                    self.change_project(&old, &new, color)
                        .map_err(|e| format!("{e:#}"))
                });
                if let Err(e) = changed {
                    // It stays open, saying why.
                    card.error = Some(e);
                    self.card = Some(card);
                } else {
                    self.refresh_sidebar();
                }
                self.request_redraw();
                return;
            }
        };
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) else {
            return;
        };
        tab.name = card.value("Name");
        if card.has("Command") {
            tab.start_command = card.value("Command");
            tab.start = card.start();
        }
        if let Err(e) = self.save_project() {
            self.set_status(Some(format!("{e:#}")));
        }
        self.update_window_title();
        self.request_redraw();
    }

    /// Opens the card of the project `name`, under its row in the sidebar: its
    /// name and color. The sidebar opens first if it isn't open.
    pub(super) fn open_project_card(&mut self, name: &str) {
        if self.sidebar.is_none() {
            self.run(Action::Sidebar);
        }
        // An open project's color is the window's (the file gets it on save).
        let open = self
            .project
            .iter()
            .chain(self.parked.iter().filter_map(|w| w.project.as_ref()));
        let color = match open.into_iter().find(|p| p.name == name) {
            Some(open) => open.color,
            None => project::load(name).ok().and_then(|p| p.color),
        };
        // Kel's soft colors, not the terminal's palette.
        let colors = &self.config.colors.project_colors;
        self.card = Some(Card::project(name, color, colors));
        self.request_redraw();
    }

    /// The sidebar again, from the files: a project renamed, closed, or another
    /// one on screen. The same row stays chosen.
    pub(super) fn refresh_sidebar(&mut self) {
        let Some(sidebar) = &self.sidebar else {
            return;
        };
        let selected = sidebar.selected();
        let current = self.project.as_ref().map(|p| p.name.as_str());
        let mut sidebar = Sidebar::new(project::sidebar_entries(current));
        sidebar.select(selected);
        self.sidebar = Some(sidebar);
    }

    /// Shift+Del in the sidebar: deletes the chosen project, open or not (see
    /// `delete_project`).
    fn sidebar_delete(&mut self) {
        let Some(sidebar) = &self.sidebar else {
            return;
        };
        if let Row::Project(p) = sidebar.row(sidebar.selected()) {
            let name = p.name.clone();
            self.delete_project(&name, "Shift+Del again");
        }
    }

    /// F2 (or a right click) in the sidebar: the chosen project's card.
    pub(super) fn sidebar_edit(&mut self) {
        let Some(sidebar) = &self.sidebar else {
            return;
        };
        if let Row::Project(p) = sidebar.row(sidebar.selected()) {
            let name = p.name.clone();
            self.open_project_card(&name);
        }
    }

    /// A key while the start screen shows: ↑↓ choose a project, Enter opens it,
    /// Esc puts the screen away. Any other key puts it away too, and goes on as
    /// usual: typing starts the shell's line, a shortcut does its thing. The
    /// sidebar, the palette and the help open over it. Returns `true` if the key was the
    /// start screen's.
    pub(super) fn start_key(&mut self, event: &KeyEvent, shortcut: Option<&Action>) -> bool {
        if matches!(
            shortcut,
            Some(Action::Sidebar | Action::Palette | Action::Help)
        ) {
            return false;
        }
        let Some(start) = &mut self.start else {
            return false;
        };
        match &event.logical_key {
            Key::Named(NamedKey::ArrowUp) => start.move_selection(-1),
            Key::Named(NamedKey::ArrowDown) => start.move_selection(1),
            Key::Named(NamedKey::Enter) if start.chosen().is_some() => {
                if let Some(action) = start.chosen() {
                    self.run(action);
                }
            }
            Key::Named(NamedKey::Escape) => self.start = None,
            // Shift, Ctrl, Alt alone: the start of a shortcut, maybe.
            Key::Named(NamedKey::Shift | NamedKey::Control | NamedKey::Alt | NamedKey::Super) => {
                return true;
            }
            _ => {
                self.start = None;
                self.request_redraw();
                return false;
            }
        }
        self.request_redraw();
        true
    }

    /// Alt+↑ (-1) or Alt+↓ (1) in the sidebar: the chosen project moves one
    /// place up or down, and the order is kept.
    fn sidebar_shift(&mut self, step: isize) {
        let Some(sidebar) = &mut self.sidebar else {
            return;
        };
        let Some(from) = sidebar.selected().checked_sub(1) else {
            return; // home
        };
        let to = from.wrapping_add_signed(step);
        if sidebar.move_project(from, to) {
            self.save_sidebar_order();
        }
    }

    /// The order the sidebar shows now goes into `order.txt`.
    pub(super) fn save_sidebar_order(&mut self) {
        let Some(sidebar) = &self.sidebar else {
            return;
        };
        if let Err(e) = project::save_order(&sidebar.names()) {
            self.set_status(Some(format!("{e:#}")));
        }
    }

    /// Del in the sidebar: closes the chosen project, if it's open (see
    /// `close_project`). The sidebar stays, and shows it closed.
    fn sidebar_close(&mut self) {
        let Some(sidebar) = &self.sidebar else {
            return;
        };
        let selected = sidebar.selected();
        let name = match sidebar.row(selected) {
            Row::Project(p) if p.current || p.live.open => p.name.clone(),
            _ => return,
        };
        self.close_project(&name, "Del again to close");
        // The project on screen may be another one now: home.
        self.refresh_sidebar();
    }

    /// Does the sidebar's chosen row, and closes it: opens that project, or the
    /// palette to save the tabs as a new one.
    pub(super) fn sidebar_choose(&mut self) {
        if let Some(sidebar) = self.sidebar.take() {
            self.run(sidebar.chosen());
            self.request_redraw();
        }
    }

    /// A key while the search is open: it's for the search, never the shell.
    /// Letters go into its box (and it finds again right away), Enter or Up goes
    /// to the match before (older output), Shift+Enter or Down to the next one,
    /// Esc closes. Ctrl+Shift+V pastes into the box.
    pub(super) fn search_key(&mut self, event: &KeyEvent, shortcut: Option<Action>) {
        let Some(search) = &mut self.search else {
            return;
        };
        let back = if self.modifiers.shift_key() { 1 } else { -1 };
        match &event.logical_key {
            _ if shortcut == Some(Action::Paste) => {
                if let Some(text) = clipboard_text() {
                    // A match is in one line: the first line of what's copied.
                    search.type_text(text.lines().next().unwrap_or(""));
                    self.find(true);
                }
            }
            Key::Named(NamedKey::Escape) => self.search = None,
            Key::Named(NamedKey::Enter) => self.find_step(back),
            Key::Named(NamedKey::ArrowUp) => self.find_step(-1),
            Key::Named(NamedKey::ArrowDown) => self.find_step(1),
            Key::Named(NamedKey::Backspace) => {
                search.backspace();
                self.find(true);
            }
            // Typed letters, not Ctrl+letter (see `palette_key`).
            _ if !self.modifiers.control_key() && !self.modifiers.super_key() => {
                if let Some(text) = event.text.as_deref() {
                    let text: String = text.chars().filter(|c| !c.is_control()).collect();
                    search.type_text(&text);
                    self.find(true);
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    /// Finds the search's text again in the tab on screen, and scrolls to the
    /// chosen match. `keep_place`: see `Search::set_found`.
    pub(super) fn find(&mut self, keep_place: bool) {
        let Some(search) = &mut self.search else {
            return;
        };
        let mut terminal = self.tabs[self.active].terminal.lock().unwrap();
        // ~600 000 letters in a full scrollback: a few ms, so on every letter.
        search.set_found(terminal.grid.find(search.query()), keep_place);
        if let Some(found) = search.current() {
            terminal.grid.show_row(found.start.0);
        }
    }

    /// The match before (-1) or after (1). Finds again first: output that came
    /// since may have new matches.
    fn find_step(&mut self, step: isize) {
        self.find(true);
        let Some(search) = &mut self.search else {
            return;
        };
        search.step(step);
        if let Some(found) = search.current() {
            let mut terminal = self.tabs[self.active].terminal.lock().unwrap();
            terminal.grid.show_row(found.start.0);
        }
    }

    /// A key while the palette is open: it's for the palette, never the shell.
    /// Letters go into the search box, Up/Down choose a line, Enter does it, Esc
    /// closes. Kel's shortcuts wait, except Ctrl+Shift+P, which closes it.
    pub(super) fn palette_key(&mut self, event: &KeyEvent) {
        let Some(palette) = &mut self.palette else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.palette = None,
            Key::Named(NamedKey::Enter) => {
                let chosen = palette.chosen();
                self.palette = None;
                if let Some(action) = chosen {
                    self.run(action);
                }
            }
            Key::Named(NamedKey::ArrowUp) => palette.move_selection(-1),
            Key::Named(NamedKey::ArrowDown) => palette.move_selection(1),
            Key::Named(NamedKey::Backspace) => palette.backspace(),
            // Typed letters, not Ctrl+letter: winit leaves Ctrl out of `text`, so
            // Ctrl+D would type a "d". Nor Cmd+letter, on a Mac.
            _ if !self.modifiers.control_key() && !self.modifiers.super_key() => {
                if let Some(text) = event.text.as_deref() {
                    let text: String = text.chars().filter(|c| !c.is_control()).collect();
                    palette.type_text(&text);
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    /// A key while the help is open: ↑↓ PgUp PgDn Home End scroll it; Esc or
    /// Ctrl+Shift+/ closes it. The palette, the search and the sidebar take its
    /// place; nothing else gets through to the shell.
    pub(super) fn help_key(&mut self, event: &KeyEvent, shortcut: Option<Action>) {
        let Some(help) = &mut self.help else {
            return;
        };
        const PAGE: isize = 10;
        match (&event.logical_key, shortcut) {
            (_, Some(Action::Help)) | (Key::Named(NamedKey::Escape), _) => self.help = None,
            (_, Some(action @ (Action::Palette | Action::Find | Action::Sidebar))) => {
                self.help = None;
                self.run(action);
            }
            (Key::Named(NamedKey::ArrowUp), _) => help.scroll_by(-1),
            (Key::Named(NamedKey::ArrowDown), _) => help.scroll_by(1),
            (Key::Named(NamedKey::PageUp), _) => help.scroll_by(-PAGE),
            (Key::Named(NamedKey::PageDown), _) => help.scroll_by(PAGE),
            (Key::Named(NamedKey::Home), _) => help.jump(false),
            (Key::Named(NamedKey::End), _) => help.jump(true),
            _ => {}
        }
        self.request_redraw();
    }
}
