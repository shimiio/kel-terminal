//! The tabs on screen: a new one, closing, showing, moving, and the window's
//! own close.

use super::{
    App, CONFIRM_TIME, Closing, FLASH_TIME, PendingClose, SWITCHER_DELAY, Switcher, TAB_DRAG_START,
};
use crate::config;
use crate::history;
use crate::keys::{self, Action};
use crate::layout::Bar;
use crate::tab::{Tab, TabId};
use std::time::Instant;
use winit::event_loop::ActiveEventLoop;
use winit::window::UserAttentionType;

impl App {
    /// Ctrl+Shift+R: the tab's command, now: one that waits for its turn, or
    /// again after it ended. Not over a program that runs: it would get the
    /// letters as keys.
    pub(super) fn run_tab_command(&mut self) {
        let tab = self.tab();
        let refused = if tab.start_command.is_none() {
            Some("no command in this tab")
        } else if tab.command.running() {
            Some("a command runs in this tab")
        } else {
            None
        };
        if let Some(refused) = refused {
            self.notice = Some((refused.to_string(), Instant::now() + CONFIRM_TIME));
            return self.request_redraw();
        }
        self.selection = None;
        self.tab_mut().run_start_command();
        self.request_redraw();
    }

    /// Opens a new tab at the end, the same size as the others and in the current
    /// tab's folder, and shows it.
    pub(super) fn new_tab(&mut self) {
        let (rows, cols) = {
            let terminal = self.tab().terminal.lock().unwrap();
            (terminal.grid.rows(), terminal.grid.cols())
        };
        let id = self.next_id;
        self.next_id += 1;
        let screen = self.screen(rows, cols);
        let cwd = self.tab().cwd();
        // It has no name yet: the project's history (see `history`).
        let project = self.project.as_ref();
        let history = project.and_then(|project| history::file(&project.name, None));
        match Tab::spawn(
            id,
            &self.shell(),
            cwd.as_deref(),
            history.as_deref(),
            self.config.suggestions,
            screen,
            self.proxy.clone(),
        ) {
            Ok(tab) => {
                self.tabs.push(tab);
                self.show_tab(self.tabs.len() - 1);
            }
            Err(e) => self.set_status(Some(format!("can't open a tab: {e:#}"))),
        }
    }

    /// A tab's program rang the bell (`on_screen`: the tab on screen). Kel in
    /// front: you see it already, or the tab's dot will say; with `flash`, the
    /// terminal flashes. Kel behind other windows: its taskbar button blinks
    /// until you come back.
    pub(super) fn ring(&mut self, on_screen: bool) {
        let bell = self.config.bell;
        if bell == config::Bell::None {
            return;
        }
        if !self.focused {
            if let Some(window) = &self.window {
                window.request_user_attention(Some(UserAttentionType::Informational));
            }
        } else if bell == config::Bell::Flash && on_screen {
            self.flash_until = Some(Instant::now() + FLASH_TIME);
            self.request_redraw();
        }
    }

    /// The shell of a new tab: the project's, or the config's.
    pub(super) fn shell(&self) -> String {
        self.project
            .as_ref()
            .and_then(|project| project.shell.clone())
            .unwrap_or_else(|| self.config.shell())
    }

    /// The shell of tab `id` ended (`exit`, or Ctrl+Shift+W): the tab goes away.
    /// The last one on screen: see `leave_empty`.
    pub(super) fn remove_tab(&mut self, id: TabId, event_loop: &ActiveEventLoop) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            self.remove_parked_tab(id);
            return;
        };
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            if !self.leave_empty() {
                event_loop.exit();
            }
            return;
        }
        // Stay on the same tab. It moved one place left if the closed tab was
        // before it. If the closed tab was the one on screen, its right neighbor
        // took its place; if it was the last one, show the new last one.
        if index < self.active || self.active == self.tabs.len() {
            self.active -= 1;
        }
        self.show_tab(self.active);
    }

    /// Shows the tab at `index` (its place in the list).
    pub(super) fn show_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.active = index;
        // Seen now.
        self.tabs[index].activity = false;
        self.tabs[index].command.done = None;
        self.update_window_title();
        // A hidden tab bar: which tab this is, for a moment.
        self.show_bar();
        // A tab came or went: the tabs and the + moved, maybe under the mouse.
        self.update_hovered();
        // The selection, the search and the half-scrolled line belonged to the
        // other tab.
        self.selection = None;
        self.search = None;
        self.scroll_rest = 0.0;
        // Another tab (a new one, a click on one): the shell, not the start screen,
        // and not a card of the tab before.
        self.start = None;
        self.card = None;
        // The link under the mouse was the other tab's: this tab's, if one is
        // there, else the hand goes with it.
        self.update_link();
        self.update_focus();
        self.request_redraw();
    }

    /// Moves the tab at `from` to the place `to`; the ones between shift over by
    /// one. The tab on screen stays on screen, wherever it goes. Nothing at the
    /// ends: the first tab doesn't go round to the last place.
    pub(super) fn move_tab(&mut self, from: usize, to: usize) {
        if from == to || to >= self.tabs.len() {
            return;
        }
        let on_screen = self.tab().id;
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = self
            .tabs
            .iter()
            .position(|tab| tab.id == on_screen)
            .unwrap_or(0);
        self.update_hovered();
        self.request_redraw();
    }

    /// The mouse moved with a tab held: once it's gone `TAB_DRAG_START` away, the
    /// tab takes the place under the mouse, one place at a time, the others
    /// stepping aside. The project saves the new order when the window closes.
    pub(super) fn drag_tab(&mut self) {
        let x = self.mouse_pos.0;
        let start = crate::layout::scaled(TAB_DRAG_START, self.layout.scale);
        let Some(drag) = &mut self.tab_drag else {
            return;
        };
        if !drag.moving && x.abs_diff(drag.from_x) < start {
            return;
        }
        drag.moving = true;
        let to = self.layout.tab_slot(x, self.tabs.len());
        self.move_tab(self.active, to);
    }

    /// Tells the programs that asked (`ESC[?1004h`) when they get the focus
    /// (`ESC[I`) and lose it (`ESC[O`). nvim does, e.g. to reload a file changed
    /// meanwhile. A tab has the focus while it's on screen and the window has it.
    pub(super) fn update_focus(&mut self) {
        let now = self.focused.then(|| self.tab().id);
        if now == self.focused_tab {
            return;
        }
        let before = std::mem::replace(&mut self.focused_tab, now);
        for (id, bytes) in [(before, b"\x1b[O"), (now, b"\x1b[I")] {
            // `before` may be gone (closed), or out of sight (another project).
            if let Some(tab) = id.and_then(|id| self.tab_anywhere(id)) {
                let asked = tab.terminal.lock().unwrap().focus_events;
                if asked {
                    tab.write(bytes);
                }
            }
        }
    }

    /// Ctrl+Shift+W: closes the tab on screen.
    pub(super) fn close_tab(&mut self) {
        self.close_tab_at(self.active, &keys::hint(&Action::CloseTab));
    }

    /// Ends the shell of the tab at `index`. If a command runs (nvim, `cargo run`),
    /// only on the second press of `how` (a key, or the tab's ×).
    pub(super) fn close_tab_at(&mut self, index: usize, how: &str) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let (id, running) = (tab.id, tab.command.running());
        let message = format!("a command runs there: {how} again to close");
        if self.confirm_close(Closing::Tab(id), running, message) {
            // Only ends the shell; the tab goes when the shell is gone (`remove_tab`),
            // the same way as after `exit`.
            self.tabs[index].kill();
        }
    }

    /// The close button or Alt+F4: saves the project and closes Kel. If a command
    /// runs in any tab, or the project can't be saved, only on the second time.
    pub(super) fn close_window(&mut self) {
        // Saved at every try: a try that only asks writes the same file again later.
        // Every project open in the window, the ones out of sight too.
        let saved = self.save_project();
        let saved = saved.and(self.save_parked());
        let running = |tabs: &[Tab]| tabs.iter().filter(|tab| tab.command.running()).count();
        let here = running(&self.tabs);
        let away: usize = self.parked.iter().map(|w| running(&w.tabs)).sum();
        let question = match (saved, here + away) {
            (Err(e), _) => Some(format!("{e:#}: close again to quit")),
            (Ok(()), 0) => None,
            (Ok(()), 1) if away == 0 => {
                Some("a command runs in a tab: close again to quit".to_string())
            }
            (Ok(()), 1) => {
                Some("a command runs in another project: close again to quit".to_string())
            }
            (Ok(()), n) if away == 0 => {
                Some(format!("commands run in {n} tabs: close again to quit"))
            }
            (Ok(()), n) => Some(format!(
                "commands run in {n} tabs, {away} in other projects: close again to quit"
            )),
        };
        let ask = question.is_some();
        if self.confirm_close(Closing::Window, ask, question.unwrap_or_default()) {
            self.quit = true;
        }
    }

    /// Returns `true` if `what` may close now: nothing would be lost (`!running`),
    /// or it's the second press within `CONFIRM_TIME`. Else asks, with `message`.
    pub(super) fn confirm_close(&mut self, what: Closing, running: bool, message: String) -> bool {
        let now = Instant::now();
        let again = self
            .pending_close
            .as_ref()
            .is_some_and(|pending| pending.what == what && now < pending.until);
        if !running || again {
            if self.pending_close.take().is_some() {
                self.request_redraw(); // the question goes away
            }
            return true;
        }
        self.pending_close = Some(PendingClose {
            what,
            message,
            until: now + CONFIRM_TIME,
        });
        self.request_redraw();
        false
    }

    /// The tab after (`step` = 1) or before (-1) the one on screen,
    /// going round from the last to the first.
    /// After Ctrl+Tab, where the tabs aren't in sight: the list of them comes
    /// if Ctrl stays held (see `switcher`). Each Ctrl+Tab has changed the tab
    /// already; the list only shows where you are among them. It goes when
    /// Ctrl is let go (see `close_switcher`).
    pub(super) fn open_switcher(&mut self) {
        let hidden = matches!(self.config.layout.tab_bar, Bar::Title | Bar::Never);
        let held = self.modifiers.control_key();
        if hidden && held && self.tabs.len() > 1 && self.switcher.is_none() {
            self.switcher = Some(Switcher {
                at: Instant::now() + SWITCHER_DELAY,
                shown: false,
                pinned: false,
            });
        }
    }

    /// The list of tabs goes: the window left, a tab chosen, Esc.
    pub(super) fn close_switcher(&mut self) {
        if self.switcher.take().is_some_and(|switcher| switcher.shown) {
            self.request_redraw();
        }
    }

    /// Ctrl let go: the list that came with Ctrl+Tab goes; a pinned one stays.
    pub(super) fn release_switcher(&mut self) {
        if self.switcher.is_some_and(|switcher| !switcher.pinned) {
            self.close_switcher();
        }
    }

    /// The list of tabs shows.
    pub(super) fn switcher_up(&self) -> bool {
        self.switcher.is_some_and(|switcher| switcher.shown)
    }

    /// A click on the title, or the palette's line: the list of tabs comes
    /// and stays (see `Switcher`), with any `tab_bar`; again, it goes.
    pub(super) fn toggle_tab_list(&mut self) {
        self.switcher = match self.switcher {
            Some(switcher) if switcher.pinned => None,
            _ => Some(Switcher {
                at: Instant::now(),
                shown: true,
                pinned: true,
            }),
        };
        self.palette = None;
        self.search = None;
        self.sidebar = None;
        self.request_redraw();
    }

    pub(super) fn next_tab(&mut self, step: isize) {
        let count = self.tabs.len() as isize;
        let index = (self.active as isize + step).rem_euclid(count);
        self.show_tab(index as usize);
    }
}
