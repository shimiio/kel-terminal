//! Kel's actions (a shortcut, a line of the palette), the clipboard, and the
//! settings: the config read again, the fonts, the zoom.

use super::{App, Closing};
use crate::config;
use crate::font::{self, Fonts};
use crate::help::Help;
use crate::keys::{self, Action};
use crate::palette::{Item, Palette};
use crate::project;
use crate::search::Search;
use crate::sidebar::Sidebar;
use crate::{open, setup};

impl App {
    /// Shows a problem in the tab bar, or (`None`) clears it.
    pub(super) fn set_status(&mut self, status: Option<String>) {
        if self.status != status {
            self.status = status;
            self.request_redraw();
        }
    }

    /// Changes the font size, then the grid to match it (see `Renderer::set_font_size`).
    fn set_font_size(&mut self, size: f32) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        renderer.set_font_size(size);
        self.cell_size_changed();
    }

    /// Changes the font family, then the grid to match it (see `Renderer::set_fonts`).
    fn set_fonts(&mut self, fonts: Fonts) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        renderer.set_fonts(fonts);
        self.cell_size_changed();
    }

    /// The UI's scale: Windows' display scale times the config's `ui_scale`,
    /// e.g. 1.5 (150%) x 1 = 1.5. Kept between a quarter and 4 times the size.
    pub(super) fn ui_scale(&self) -> f32 {
        let display = self
            .window
            .as_ref()
            .map_or(1.0, |w| w.scale_factor() as f32);
        (display * self.config.ui_scale).clamp(0.25, 4.0)
    }

    /// After the cell got a new size: fit the grid to the window again.
    pub(super) fn cell_size_changed(&mut self) {
        // The selection points at cells that move around when the grid changes size.
        self.selection = None;
        self.on_resize();
        self.request_redraw();
    }

    /// Loads the config file again if it was saved since the last look, and applies it.
    /// A broken file keeps the old settings and shows the error in the window title.
    pub(super) fn reload_config_if_changed(&mut self) {
        let modified = config::modified();
        if modified == self.config_modified {
            return;
        }
        self.config_modified = modified;

        // `Some`: something went wrong, and the tab bar says what.
        let error = match config::load() {
            Ok(mut new) => {
                let mut error = None;
                // Only what changed, so saving another setting doesn't undo your zoom.
                if new.font != self.config.font {
                    match font::load_family(&new.font) {
                        Ok(fonts) => self.set_fonts(fonts),
                        Err(e) => error = Some(format!("{e:#}")), // keeps the old font
                    }
                }
                if new.font_size != self.config.font_size {
                    self.set_font_size(new.font_size);
                }
                if new.line_height != self.config.line_height
                    && let Some(renderer) = &mut self.renderer
                {
                    renderer.set_line_height(new.line_height);
                    self.cell_size_changed();
                }
                if new.layout != self.config.layout || new.ui_scale != self.config.ui_scale {
                    self.config.layout = new.layout.clone();
                    self.config.ui_scale = new.ui_scale;
                    self.cell_size_changed(); // not the cell, but the room for cells
                }
                if new.pixel_name != self.config.pixel_name
                    && let Some(renderer) = &mut self.renderer
                {
                    renderer.set_pixel_name(new.pixel_name);
                    self.on_resize(); // the name's width: the tabs move
                    self.request_redraw();
                }
                if new.cursor != self.config.cursor
                    && let Some(renderer) = &mut self.renderer
                {
                    renderer.set_cursor(new.cursor);
                    self.request_redraw();
                }
                if new.colors != self.config.colors
                    && let Some(renderer) = &mut self.renderer
                {
                    renderer.set_theme(new.colors.clone());
                    self.request_redraw();
                }
                // The effect only after a restart: Windows' glass needs the window
                // see-through from its start. Its opacity and grain right away.
                new.glass.effect = self.config.glass.effect;
                if new.glass != self.config.glass {
                    if let Some(renderer) = &mut self.renderer {
                        renderer.set_glass(new.glass.clone());
                    }
                    self.request_redraw();
                }
                #[cfg(target_os = "macos")]
                if new.option_as_alt != self.config.option_as_alt
                    && let Some(window) = &self.window
                {
                    use winit::platform::macos::WindowExtMacOS;
                    window.set_option_as_alt(super::option_as_alt(new.option_as_alt));
                }
                // Every tab keeps the new number of lines, the ones behind too.
                if new.scrollback != self.config.scrollback {
                    for tab in self.all_tabs() {
                        let mut terminal = tab.terminal.lock().unwrap();
                        terminal.grid.set_scrollback_limit(new.scrollback);
                    }
                    self.request_redraw();
                }
                // Another picture: shown at once, if the start screen is up.
                // Turned off: it goes now.
                let backdrop = new.backdrop != self.config.backdrop;
                let start = new.start_screen != self.config.start_screen;
                self.config = new;
                if (backdrop || start) && self.start.is_some() {
                    self.start = self.start_screen();
                    self.request_redraw();
                }
                error
            }
            // e.g. "config.toml, line 3: invalid type: string "big", expected f32"
            Err(e) => Some(format!("{e:#}")),
        };
        self.set_status(error);
    }

    /// Does what a shortcut stands for. Which key is which action: `keys::shortcut`.
    pub(super) fn run(&mut self, action: Action) {
        match action {
            Action::NewTab => self.new_tab(),
            Action::CloseTab => self.close_tab(),
            Action::NextTab => self.next_tab(1),
            Action::PreviousTab => self.next_tab(-1),
            Action::GoToTab(n) => self.show_tab(n - 1), // tab 1 is at index 0
            Action::LastTab => self.show_tab(self.tabs.len() - 1),
            Action::MoveTabLeft => self.move_tab(self.active, self.active.saturating_sub(1)),
            Action::MoveTabRight => self.move_tab(self.active, self.active + 1),
            Action::Paste => self.paste(),
            Action::Copy => self.copy(),
            Action::Interrupt => self.send_to_shell(&[0x03]), // the byte Ctrl+C stands for
            Action::RunTabCommand => self.run_tab_command(),
            Action::ZoomIn => self.zoom(1.0),
            Action::ZoomOut => self.zoom(-1.0),
            Action::ZoomReset => self.set_font_size(self.config.font_size),
            Action::OpenConfig => {
                if let Ok(path) = config::path() {
                    let _ = open::text_file(&path);
                }
            }
            Action::Find => {
                self.search = match self.search {
                    Some(_) => None,
                    None => Some(Search::new()),
                };
                self.palette = None;
                self.sidebar = None;
                self.request_redraw();
            }
            Action::Sidebar => {
                self.sidebar_peek = false;
                self.sidebar = match self.sidebar {
                    Some(_) => None,
                    None => {
                        let current = self.project.as_ref().map(|p| p.name.as_str());
                        Some(Sidebar::new(project::sidebar_entries(current)))
                    }
                };
                self.palette = None;
                self.search = None;
                self.link = None;
                self.request_redraw();
            }
            Action::Palette => {
                self.search = None;
                self.sidebar = None;
                self.palette = match self.palette {
                    Some(_) => None,
                    None => {
                        // Kel's actions first, then the projects, then the install scripts.
                        let current = self.project.as_ref().map(|p| p.name.as_str());
                        let mut items = keys::palette_items();
                        items.push(Item {
                            label: "Edit tab: name and command".to_string(),
                            hint: "double-click a tab".to_string(),
                            action: Action::EditTab,
                        });
                        // The project on screen can be changed, and closed.
                        if let Some(name) = current {
                            items.push(Item {
                                label: format!("Edit project: {name}"),
                                hint: "name and color".to_string(),
                                action: Action::EditProject,
                            });
                        }
                        if let Some(name) = current {
                            items.push(Item {
                                label: format!("Close project: {name}"),
                                hint: "then home".to_string(),
                                action: Action::CloseProject,
                            });
                            items.push(Item {
                                label: format!("Delete project: {name}"),
                                hint: "its file, not the folder".to_string(),
                                action: Action::DeleteProject,
                            });
                        }
                        items.extend(project::palette_items(current));
                        items.extend(setup::palette_items());
                        // And from what's typed: "Save the tabs as project: <it>",
                        // in the folder of the tab on screen.
                        let existing = project::dir()
                            .map(|dir| project::names_in(&dir))
                            .unwrap_or_default();
                        let palette = match self.tab().cwd() {
                            Some(folder) => Palette::new(items).with_typed_item(move |typed| {
                                project::save_item(typed, &folder, &existing)
                            }),
                            None => Palette::new(items),
                        };
                        Some(palette)
                    }
                };
                self.request_redraw();
            }
            Action::Help => {
                self.help = match self.help {
                    Some(_) => None,
                    None => Some(Help::new(&self.config)),
                };
                self.palette = None;
                self.search = None;
                self.sidebar = None;
                self.card = None;
                self.request_redraw();
            }
            Action::Install(name) => self.install(name),
            Action::OpenProject(name) => self.switch_to(Some(&name)),
            Action::OpenHome => self.switch_to(None),
            Action::NextProject => self.step_project(1),
            Action::PreviousProject => self.step_project(-1),
            Action::EditTab => self.open_tab_card(self.active),
            Action::EditProject => {
                if let Some(name) = self.project.as_ref().map(|p| p.name.clone()) {
                    self.open_project_card(&name);
                }
            }
            Action::CloseProject => {
                if let Some(name) = self.project.as_ref().map(|p| p.name.clone()) {
                    self.close_project(&name, "choose Close project again");
                }
            }
            Action::DeleteProject => {
                if let Some(name) = self.project.as_ref().map(|p| p.name.clone()) {
                    self.delete_project(&name, "choose Delete project again");
                }
            }
            Action::SaveProject(name) => self.save_as_project(name),
            Action::Quit => self.close_window(),
            Action::Hide => self.hide = true,
            Action::Minimize => {
                if let Some(window) = &self.window {
                    window.set_minimized(true);
                }
            }
        }
    }

    /// Runs the install script `name` in a new tab: Kel writes it to the temp
    /// folder, and the tab's shell runs it once it's ready.
    fn install(&mut self, name: &str) {
        let Some(script) = setup::find(name) else {
            return;
        };
        let path = match setup::write_to_temp(script) {
            Ok(path) => path,
            Err(e) => {
                self.set_status(Some(format!("can't write the {name} script: {e}")));
                return;
            }
        };
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                self.set_status(Some(format!("can't tell where kel.exe is: {e}")));
                return;
            }
        };
        let tabs = self.tabs.len();
        self.new_tab();
        if self.tabs.len() > tabs {
            let tab = self.tabs.last_mut().unwrap();
            tab.type_when_ready(&setup::command(script, &path, &exe));
        }
    }

    /// Pastes the clipboard into the shell. A program that takes pasted text as
    /// a whole (bracketed paste: nvim, a newer PSReadLine) gets it as it is.
    /// Else each line would run as it comes: one line goes in without its line
    /// break, and several ask first, in the tab bar ("paste 2 lines?"), as
    /// Windows Terminal does.
    fn paste(&mut self) {
        let Some(text) = clipboard_text() else {
            return;
        };
        let bracketed = self.tab().terminal.lock().unwrap().bracketed_paste;
        if bracketed {
            self.send_to_shell(&keys::paste_bytes(&text, true));
            return;
        }
        let (text, lines) = keys::plain_paste(&text);
        let again = keys::hint(&Action::Paste);
        let message = format!("Paste {lines} lines? They run. {again} again");
        let what = Closing::Paste(text.to_string());
        if self.confirm_close(what, lines > 1, message) {
            self.send_to_shell(&keys::paste_bytes(text, false));
        }
    }

    /// Copies the mouse selection. Without one, it's a Ctrl+C for the shell,
    /// and PowerShell then copies its own keyboard selection.
    fn copy(&mut self) {
        if !self.copy_selection() {
            // Cmd+C on a Mac only copies; Ctrl+Shift+C elsewhere is Ctrl+C then.
            if !keys::MAC {
                self.send_to_shell(&[0x03]);
            }
            return;
        }
        self.selection = None;
        self.request_redraw();
    }

    /// Puts the mouse selection's text in the clipboard, if there is one. It
    /// stays selected: `copy` takes it away, a selection copied as the mouse
    /// lets go (`copy_on_select`) stays to be seen.
    pub(super) fn copy_selection(&mut self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        let (start, end) = selection.ordered();
        let text = self
            .tab()
            .terminal
            .lock()
            .unwrap()
            .grid
            .text_between(start, end);
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(text);
        }
        true
    }

    /// Font one pixel bigger (`step` = 1) or smaller (-1).
    fn zoom(&mut self, step: f32) {
        if let Some(renderer) = &self.renderer {
            self.set_font_size(renderer.font_size() + step);
        }
    }
}

/// What's in the clipboard, as text to paste: the text, or else the paths of
/// files copied in Explorer (see `keys::paths_text`). `None` for anything else,
/// a picture too: Claude Code takes one itself, on Alt+V.
pub(super) fn clipboard_text() -> Option<String> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    if let Ok(text) = clipboard.get_text() {
        return Some(text);
    }
    let paths = clipboard.get().file_list().ok()?;
    (!paths.is_empty()).then(|| keys::paths_text(&paths))
}
