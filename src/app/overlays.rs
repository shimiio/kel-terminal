//! What opens over the terminal and takes the keys: the sidebar, the cards,
//! the search, the ask box, the palette, the start screen, the help.

use super::{App, CONFIRM_TIME, UserEvent, actions::clipboard_text};
use crate::ask::{self, Answer, Around, Ask};
use crate::card::{self, Card, Target};
use crate::config;
use crate::history;
use crate::keys::{self, Action};
use crate::layout::Rect;
use crate::ollama;
use crate::project;
use crate::settings::{self, Settings};
use crate::sidebar::{Row, Sidebar};
use std::thread;
use std::time::{Duration, Instant};
use winit::event::KeyEvent;
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

/// How long Kel says what's doubtful in a command it put in the line: time to
/// read a sentence, and the command.
const DOUBT_TIME: Duration = Duration::from_secs(10);

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
        self.ask = None;
        self.sidebar = None;
        self.request_redraw();
    }

    /// A key while the list of tabs shows (see `Switcher`): what the tab bar's
    /// mouse does where there's no tab bar. ↑ ↓ go to the tab above or below,
    /// with Shift they move the tab on screen there; E opens its card (its
    /// name, its command). A pinned list: Esc or Enter puts it away, and so
    /// does any other key, which then does what it always does. One that came
    /// with Ctrl+Tab: Enter is the card too. `false`: not the list's key.
    pub(super) fn switcher_key(&mut self, event: &KeyEvent) -> bool {
        let pinned = self.switcher.is_some_and(|switcher| switcher.pinned);
        let PhysicalKey::Code(code) = event.physical_key else {
            return false;
        };
        let step: isize = match code {
            KeyCode::ArrowUp => -1,
            KeyCode::ArrowDown => 1,
            KeyCode::Escape | KeyCode::Enter if pinned => {
                self.close_switcher();
                return true;
            }
            KeyCode::KeyE | KeyCode::Enter => {
                self.close_switcher();
                self.open_tab_card(self.active);
                return true;
            }
            // Shift on its way to Shift+↑ isn't a key elsewhere.
            _ if matches!(
                event.logical_key,
                Key::Named(NamedKey::Shift | NamedKey::Control | NamedKey::Alt | NamedKey::Super)
            ) =>
            {
                return false;
            }
            _ => {
                if pinned {
                    self.close_switcher();
                }
                return false;
            }
        };
        if self.modifiers.shift_key() {
            // Not past the ends: `move_tab` does nothing there.
            let to = self.active.saturating_add_signed(step);
            self.move_tab(self.active, to);
        } else {
            self.next_tab(step);
        }
        true
    }

    /// Where the open card is: under its tab.
    pub(super) fn card_rect(&self) -> Option<Rect> {
        let card = self.card.as_ref()?;
        let (window, scale) = (self.layout.window, self.layout.scale);
        let anchor = match &card.target {
            Target::Tab(id) => {
                let index = self.tabs.iter().position(|tab| tab.id == *id)?;
                self.layout.tab_anchor(index, self.tabs.len())
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
        let old = std::mem::replace(&mut tab.name, card.value("Name"));
        let new = tab.name.clone();
        if card.has("Command") {
            tab.start_command = card.value("Command");
            tab.start = card.start();
        }
        // A project's tab renamed: its history of commands goes with the name,
        // unless another tab still has the old one. The shell running in it
        // writes to the old file until it starts again (see `history`).
        if let (Some(project), Some(old), Some(new)) = (&self.project, &old, &new)
            && old != new
            && !self.tabs.iter().any(|tab| tab.name.as_ref() == Some(old))
        {
            history::rename_tab(&project.name, old, new);
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

    /// Ctrl+Shift+A: the box that asks a model for a command opens (see `ask`),
    /// or closes. Not while a program has the keys (nvim, a running command):
    /// the answer goes into the shell's line, and there's none.
    pub(super) fn toggle_ask(&mut self) {
        if self.ask.take().is_some() {
            return self.request_redraw();
        }
        let tab = self.tab();
        if tab.command.running() || tab.terminal.lock().unwrap().grid.alternate() {
            let why = "A program runs here: no line to put a command in";
            self.notice = Some((why.to_string(), Instant::now() + CONFIRM_TIME));
            return self.request_redraw();
        }
        self.ask = Some(Ask::new());
        self.palette = None;
        self.search = None;
        self.sidebar = None;
        // The model into memory while the question is typed: read from the
        // disk it takes ~15 s, which the first answer would wait for.
        let ai = self.config.ai.clone();
        thread::spawn(move || ollama::load(&ai.address, &ai.model));
        self.request_redraw();
    }

    /// A key while the ask box is open: it's for the box, never the shell.
    /// Letters go into it, Enter asks, Esc closes (an answer on its way is
    /// dropped). While the model is asked, only Esc. Ctrl+Shift+V pastes.
    pub(super) fn ask_key(&mut self, event: &KeyEvent, shortcut: Option<Action>) {
        let Some(ask) = &mut self.ask else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.ask = None,
            _ if ask.waiting() => {}
            _ if shortcut == Some(Action::Paste) => {
                if let Some(text) = clipboard_text() {
                    // A question is one line: the first of what's copied.
                    ask.type_text(text.lines().next().unwrap_or(""));
                }
            }
            Key::Named(NamedKey::Enter) => self.send_ask(),
            Key::Named(NamedKey::Backspace) => ask.backspace(),
            // Typed letters, not Ctrl+letter (see `palette_key`).
            _ if !self.modifiers.control_key() && !self.modifiers.super_key() => {
                if let Some(text) = event.text.as_deref() {
                    let text: String = text.chars().filter(|c| !c.is_control()).collect();
                    ask.type_text(&text);
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    /// Enter in the ask box: the question goes to the model, with what's around
    /// it in the tab on screen (its shell, folder, last commands). On a thread of its own: the answer takes
    /// a second or more, and comes back as `UserEvent::Answer`.
    fn send_ask(&mut self) {
        let Some(ask) = &mut self.ask else {
            return;
        };
        let question = ask.query().trim().to_string();
        if question.is_empty() {
            return;
        }
        self.asked += 1;
        let number = self.asked;
        ask.sent(number);
        let ai = self.config.ai.clone();
        let tab = self.tab();
        let around = Around {
            shell: tab.shell().to_string(),
            folder: tab.cwd(),
            commands: tab.command.recent.iter().cloned().collect(),
        };
        let proxy = self.proxy.clone();
        thread::spawn(move || {
            let answer = ask::answer(&ai, &around, &question);
            // Err: Kel closed meanwhile.
            let _ = proxy.send_event(UserEvent::Answer(number, answer));
        });
    }

    /// The model's answer to question `number`. A command: the box closes, and
    /// it's in the shell's line as if pasted, not run: Enter is yours. One the
    /// check still finds something in, after the second try: that's said where
    /// Kel says things, for longer than a question waits. None: the box says
    /// why. An answer nobody waits for anymore (Esc, another tab) is dropped.
    pub(super) fn on_answer(&mut self, number: u64, answer: Result<Answer, String>) {
        let Some(ask) = self.ask.as_mut().filter(|ask| ask.waits_for(number)) else {
            return;
        };
        match answer {
            Ok(answer) => {
                self.ask = None;
                let bracketed = {
                    let mut terminal = self.tab().terminal.lock().unwrap();
                    terminal.grid.scroll_to_bottom(); // where the line is
                    terminal.bracketed_paste
                };
                // One line (see `ask::command`): nothing in it runs it.
                self.send_to_shell(&keys::paste_bytes(&answer.command, bracketed));
                if let Some(doubt) = answer.doubt {
                    let until = Instant::now() + DOUBT_TIME;
                    self.notice = Some((format!("Look at it: {doubt}"), until));
                }
            }
            Err(why) => ask.failed(why),
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

    /// Ctrl+,: the settings panel opens (see `settings`), over the config file as
    /// it is now. A file Kel can't read has no panel: the file opens, to be fixed,
    /// and the tab bar says what's wrong with it.
    pub(super) fn open_settings(&mut self) {
        match config::text().and_then(|text| Settings::new(text, &self.config)) {
            Ok(settings) => {
                self.settings = Some(settings);
                self.palette = None;
                self.search = None;
                self.ask = None;
                self.sidebar = None;
                self.card = None;
                self.help = None;
            }
            Err(e) => {
                self.set_status(Some(format!("{e:#}")));
                self.run(Action::OpenConfig);
            }
        }
        self.request_redraw();
    }

    /// A key while the settings panel is open: it's for the panel, never the
    /// shell. ↑↓ choose, ←→ change the chosen one (Kel shows it at once), Enter
    /// keeps the changes, Esc takes them back. Ctrl+, again, or Enter on the
    /// last line: the file opens, with the changes in it.
    pub(super) fn settings_key(&mut self, event: &KeyEvent, shortcut: Option<Action>) {
        let Some(settings) = &mut self.settings else {
            return;
        };
        match (&event.logical_key, shortcut) {
            (Key::Named(NamedKey::Escape), _) => self.close_settings(false),
            (_, Some(Action::Settings)) => {
                self.close_settings(true);
                self.run(Action::OpenConfig);
            }
            (Key::Named(NamedKey::Enter), _) => {
                let open_file = settings.chosen() == settings::Line::OpenFile;
                self.close_settings(true);
                if open_file {
                    self.run(Action::OpenConfig);
                }
            }
            (Key::Named(NamedKey::ArrowUp), _) => settings.move_selection(-1),
            (Key::Named(NamedKey::ArrowDown), _) => settings.move_selection(1),
            (Key::Named(NamedKey::ArrowLeft), _) => self.change_setting(-1),
            (Key::Named(NamedKey::ArrowRight), _) => self.change_setting(1),
            _ => {}
        }
        self.request_redraw();
    }

    /// ← (-1) or → (1): the chosen setting's next value, and Kel shows it, as
    /// if the file were saved with it.
    fn change_setting(&mut self, step: isize) {
        let Some(settings) = &mut self.settings else {
            return;
        };
        if !settings.change(step) {
            return;
        }
        let error = match settings.preview() {
            Ok(config) => self.apply_config(config),
            Err(e) => Some(format!("{e:#}")),
        };
        self.set_status(error);
    }

    /// The panel closes. `keep`: its changes go into the file, which Kel then
    /// finds as it has them (nothing more to apply). Else, or if the file can't
    /// be written, Kel goes back to the config it had: what's on screen is
    /// always what's in the file.
    fn close_settings(&mut self, keep: bool) {
        let Some(settings) = self.settings.take() else {
            return;
        };
        let saved = match keep {
            true => config::save_settings(settings.changes()),
            false => Ok(()),
        };
        let mut error = None;
        if !keep || saved.is_err() {
            error = self.apply_config(settings.before().clone());
        }
        if let Err(e) = saved {
            error = Some(format!("The settings weren't saved: {e:#}"));
        }
        self.set_status(error);
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
