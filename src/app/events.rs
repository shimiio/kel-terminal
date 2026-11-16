//! What winit tells Kel: the window is there, a shell said something, a key,
//! a resize, time to draw. Each kind of event has its `on_...` here; the mouse's
//! are in `mouse`.

use super::{App, BLINK_INTERVAL, COLS, IconKey, PEEK_DELAY, ROWS, UserEvent};
use crate::keys::{self, Action};
use crate::renderer::{Chrome, Renderer, TabLabel};
use crate::{glass, icons, logo};
use std::rc::Rc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Icon, Window, WindowId};

// winit calls these methods when something happens; we never call them ourselves.
impl ApplicationHandler<UserEvent> for App {
    /// Called when the app starts. A window can only be created from here.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(fonts) = self.fonts.take() else {
            return; // already started
        };
        // No Windows title bar: Kel draws its own (the tabs, the buttons).
        let attrs = Window::default_attributes()
            .with_title("Kel") // still shown in the taskbar
            .with_decorations(false)
            // See-through only from the start: Windows' glass needs it.
            .with_transparent(self.config.glass.on());
        #[cfg(target_os = "windows")]
        let attrs = {
            use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
            // Without the title bar, Windows also drops the window's shadow: ask for it
            // back. And round corners, like other windows on Windows 11.
            attrs
                .with_undecorated_shadow(true)
                .with_corner_preference(CornerPreference::Round)
        };
        // On Linux, the name a dock finds `kel.desktop` by (its StartupWMClass,
        // Wayland's app id): Kel's icon and name there, not a blank one.
        #[cfg(all(unix, not(target_os = "macos")))]
        let attrs = {
            use winit::platform::x11::WindowAttributesExtX11;
            attrs.with_name("kel", "kel")
        };
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs.with_option_as_alt(super::option_as_alt(self.config.option_as_alt))
        };
        let window = Rc::new(event_loop.create_window(attrs).unwrap());
        if self.config.glass.on() {
            glass::apply(
                &window,
                self.config.glass.effect,
                self.config.colors.background,
            );
        }
        // The cell size comes from the font, which the renderer holds, and the renderer
        // needs the window. So: window first, then renderer, then the size for
        // 80x24 cells plus the spaces around them.
        let theme = self.config.colors.clone();
        let text_scale = window.scale_factor() as f32;
        let font_size = self.config.font_size;
        let glass = self.config.glass.clone();
        let mut renderer =
            Renderer::new(window.clone(), fonts, font_size, text_scale, theme, glass).unwrap();
        renderer.set_pixel_name(self.config.pixel_name);
        renderer.set_cursor(self.config.cursor);
        renderer.set_line_height(self.config.line_height);
        let (cell_w, cell_h) = renderer.cell_size();
        self.window = Some(window.clone());
        let ui_scale = self.ui_scale();
        let (w, h) = self
            .config
            .layout
            .window_size(COLS * cell_w, ROWS * cell_h, ui_scale);
        let _ = window.request_inner_size(PhysicalSize::new(w as u32, h as u32));
        self.renderer = Some(renderer);
        self.on_resize();
    }

    /// Our own events, sent by the tabs' threads.
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            // Only the tab on screen needs a redraw. The others keep their new
            // output in their grids, ready for when you switch to them, and
            // light a dot in the tab bar.
            UserEvent::PtyOutput(id) => {
                let on_screen = id == self.tab().id;
                let Some(tab) = self.tab_anywhere(id) else {
                    return;
                };
                // The output can hold a new title (`ESC]0;...`), for any tab.
                let renamed = tab.refresh_title();
                // A project's command waits for the shell's first prompt.
                tab.type_startup_when_ready();
                // And command marks (`ESC]133;...`).
                let marked = tab.refresh_command(on_screen);
                let lit = !on_screen && tab.mark_activity();
                // A command ended in a tab out of sight: its mark is news.
                let ended = marked && !on_screen && tab.command.done.is_some();
                if tab.take_bell() {
                    self.ring(on_screen);
                }
                if ended && self.tabs.iter().any(|tab| tab.id == id) {
                    self.show_bar();
                }
                if on_screen && renamed {
                    self.update_window_title();
                }
                if on_screen || renamed || marked || lit {
                    self.request_redraw();
                }
            }
            // Not `send_to_shell`: an answer isn't typing, so the mouse selection stays.
            UserEvent::Reply(id, bytes) => {
                if let Some(tab) = self.tab_anywhere(id) {
                    tab.write(&bytes);
                }
            }
            UserEvent::PtyExited(id) => self.remove_tab(id, event_loop),
            UserEvent::Answer(number, answer) => self.on_answer(number, answer),
        }
    }

    /// Every window event arrives here and is passed to its own handler below.
    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            // Alt+F4, or "Close window" in the taskbar.
            WindowEvent::CloseRequested => self.close_window(),
            WindowEvent::RedrawRequested => self.on_redraw(),
            WindowEvent::Resized(_) => self.on_resize(),
            // The window moved to a display with another scale (or the setting changed).
            // Windows then resizes the window too, and `Resized` follows.
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.set_text_scale(scale_factor as f32);
                }
                self.cell_size_changed();
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                self.update_link(); // Ctrl pressed or let go over a link
                if !self.modifiers.control_key() {
                    self.release_switcher();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.on_key(event)
            }
            WindowEvent::MouseWheel { delta, .. } => self.on_mouse_wheel(delta),
            WindowEvent::CursorMoved { position, .. } => self.on_mouse_move(position),
            WindowEvent::CursorLeft { .. } => {
                self.mouse_inside = false;
                self.update_hovered();
                self.leave_peek();
                self.update_link();
                if let Some(start) = &mut self.start
                    && start.hover(None)
                {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse_button(state, button),
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if !focused {
                    self.close_switcher(); // Ctrl's release won't come here
                }
                self.update_focus();
                self.restart_blink();
                self.request_redraw(); // the cursor: hollow, or not
            }
            _ => {}
        }
    }

    /// Called when winit has handled all events and is about to sleep.
    /// We use it as the timer for the blinking cursor.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.quit {
            event_loop.exit();
            return;
        }
        if std::mem::take(&mut self.hide) {
            #[cfg(target_os = "macos")]
            {
                use winit::platform::macos::ActiveEventLoopExtMacOS;
                event_loop.hide_application();
            }
        }
        if Instant::now() >= self.next_blink {
            // `cursor_blink = false`: always on, so nothing to draw again.
            let on = !self.cursor_on || !self.config.cursor_blink;
            if on != self.cursor_on {
                self.cursor_on = on;
                self.request_redraw();
            }
            self.next_blink = Instant::now() + BLINK_INTERVAL;
            // The same timer looks for a saved config file, twice a second,
            // and lets an unanswered "close?" go.
            self.reload_config_if_changed();
            if self
                .pending_close
                .as_ref()
                .is_some_and(|pending| Instant::now() >= pending.until)
            {
                self.pending_close = None;
                self.request_redraw(); // a hidden tab bar came for it: it goes
            }
            if self
                .notice
                .as_ref()
                .is_some_and(|(_, until)| Instant::now() >= *until)
            {
                self.notice = None;
                self.request_redraw();
            }
        }
        self.peek();
        // Ctrl held long enough after Ctrl+Tab: the list of tabs shows.
        if let Some(switcher) = &mut self.switcher
            && !switcher.shown
            && Instant::now() >= switcher.at
        {
            switcher.shown = true;
            self.request_redraw();
        }
        // A hidden tab bar has shown long enough.
        if self.bar_until.is_some_and(|until| Instant::now() >= until) {
            self.bar_until = None;
            self.request_redraw();
        }
        // The bell's flash is over.
        if self
            .flash_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.flash_until = None;
            self.request_redraw();
        }
        // Sleep until the next event, or until it's time to blink (or to open the
        // sidebar, the mouse resting on the sidebar button, or to end a flash),
        // whichever comes first.
        let wake = [
            self.burger_since.map(|since| since + PEEK_DELAY),
            self.flash_until,
            self.bar_until,
            self.switcher.filter(|s| !s.shown).map(|s| s.at),
        ]
        .into_iter()
        .flatten()
        .fold(self.next_blink, Instant::min);
        event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
    }
}

// ---------------------------------------------------------------------------
// Event handlers, one per kind of event.
// ---------------------------------------------------------------------------
impl App {
    /// The tab on screen wants a solid background (see `Chrome::solid`): a
    /// full-screen program runs in it, any; or one of `solid_for` (Claude
    /// Code), which draw in the normal screen. Those only while a command runs:
    /// not a tab only called "claude".
    fn solid_under(&self) -> bool {
        let glass = &self.config.glass;
        if !glass.solid_under_apps {
            return false;
        }
        let tab = self.tab();
        let full_screen = tab.terminal.lock().unwrap().grid.alternate();
        full_screen
            || tab.command.running() && glass.wants_solid(tab.command.line.as_deref(), &tab.title)
    }

    /// The window's own title, seen in the taskbar and in Alt+Tab: the tab on screen.
    /// Also on each draw, as a tab's label changes when its command gets busy:
    /// set only when it's different.
    pub(super) fn update_window_title(&mut self) {
        if let Some(window) = &self.window {
            let title = match &self.project {
                Some(project) => format!("{} - {} - Kel", self.tab().label(), project.name),
                None => format!("{} - Kel", self.tab().label()),
            };
            if title != self.window_title {
                window.set_title(&title);
                self.window_title = title;
            }
        }
    }

    /// Shows the cursor and starts the blink over, so it doesn't vanish while you type.
    fn restart_blink(&mut self) {
        self.cursor_on = true;
        self.next_blink = Instant::now() + BLINK_INTERVAL;
        self.request_redraw();
    }

    pub(super) fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Draws the grid (and the mouse selection) into the window.
    /// The blink timer redraws twice a second, so a ring for a busy tab shows up
    /// at most half a second after `BUSY_DELAY`, without a timer of its own.
    fn on_redraw(&mut self) {
        self.update_icon();
        self.update_window_title();
        if self.minimized() {
            return; // nothing to see
        }
        // What's under the mouse can change without the mouse moving (new
        // output, a scroll, the start screen coming up): the link is looked
        // for again, so the hand never stays over what's no link anymore.
        self.update_link();
        // The same for the title bar: a hidden tab bar comes and goes under
        // the mouse.
        self.update_hovered();
        // What's going on in each project, now: a few tabs looked at.
        if let Some(mut sidebar) = self.sidebar.take() {
            sidebar.update_live(|name| self.live_of(name));
            sidebar.fit(self.layout.sidebar(), self.layout.scale);
            self.sidebar = Some(sidebar);
        }
        if let Some(mut start) = self.start.take() {
            start.update_live(|name| self.live_of(name));
            self.start = Some(start);
        }
        // Home's start screen up: its tab says so, "home" and Kel's mark. A key
        // puts the screen away, and the tab is the shell's again.
        let start = self.start.is_some().then_some(self.active);
        let labels: Vec<TabLabel> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| TabLabel {
                title: if start == Some(i) {
                    "home"
                } else {
                    tab.label()
                },
                icon: if start == Some(i) {
                    icons::KEL
                } else {
                    tab.icon()
                },
                activity: tab.activity,
                busy: tab.command.busy(),
                done: tab.command.done,
            })
            .collect();
        let chrome = Chrome {
            tabs: &labels,
            active: self.active,
            hovered: self.hovered,
            mouse_on_bar: self.mouse_on_bar,
            bar_up: self.bar_up(),
            switcher: self.switcher_up(),
            maximized: self.window.as_ref().is_some_and(|w| w.is_maximized()),
            status: self.status.as_deref(),
            question: self
                .pending_close
                .as_ref()
                .map(|p| p.message.as_str())
                .or(self.notice.as_ref().map(|(text, _)| text.as_str())),
            palette: self.palette.as_ref(),
            search: self.search.as_ref(),
            ask: self.ask.as_ref(),
            project: self.project.as_ref().map(|p| (p.name.as_str(), p.color)),
            link: self.link.as_ref().map(|link| link.found),
            link_url: self
                .link
                .as_ref()
                .filter(|link| link.marked)
                .map(|link| link.url.as_str()),
            sidebar: self.sidebar.as_ref(),
            start: self.start.as_ref(),
            card: self.card.as_ref().zip(self.card_rect()),
            help: self.help.as_ref(),
            keys_here: self.keys_here(),
            solid: self.solid_under(),
            flash: self.flash_until.is_some(),
        };
        if let Some(renderer) = &mut self.renderer {
            let started = Instant::now();
            renderer.draw(
                &self.tabs[self.active].terminal.lock().unwrap().grid,
                &self.layout,
                &chrome,
                self.selection,
                self.cursor_on,
            );
            self.frames.add(started.elapsed());
        }
    }

    /// Whether the keys go to the terminal: the window has them, and nothing
    /// over the terminal takes them (see `on_key`). Else its cursor is hollow.
    fn keys_here(&self) -> bool {
        self.focused
            && self.sidebar.is_none()
            && self.palette.is_none()
            && self.search.is_none()
            && self.ask.is_none()
            && self.card.is_none()
            && self.help.is_none()
    }

    /// The window's icon (`k▌`, see `logo::icon`), its cursor the color of the
    /// project on screen: none in home, the text's. Made again only when one of
    /// its colors changed (another project, its color edited, another theme), so
    /// every redraw can ask.
    fn update_icon(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let theme = &self.config.colors;
        let cursor = self.project.as_ref().and_then(|p| p.color);
        let key = IconKey {
            letters: theme.foreground,
            cursor: theme.accent_for(cursor),
            back: theme.background,
            // 16 at 100%: the small one, by the title; the taskbar's twice that.
            size: (16.0 * window.scale_factor()).round() as usize,
        };
        if self.icon == Some(key) {
            return;
        }
        self.icon = Some(key);
        let make = |size: usize| {
            let rgba = logo::icon(size, key.letters, key.cursor, key.back);
            Icon::from_rgba(rgba, size as u32, size as u32).ok()
        };
        window.set_window_icon(make(key.size));
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowExtWindows;
            window.set_taskbar_icon(make(key.size * 2));
        }
    }

    /// Whether the window is minimized (only in the taskbar).
    fn minimized(&self) -> bool {
        self.window
            .as_ref()
            .is_some_and(|w| w.is_minimized() == Some(true))
    }

    /// The window changed size: resize the grid and tell the shell.
    /// Uses the window's *current* size, because the event's size can be outdated.
    pub(super) fn on_resize(&mut self) {
        // Minimized, Kel's window is 160 x 28 pixels: no room for anything. The
        // layout and the grids stay as they were; they're right again on restore.
        if self.minimized() {
            return;
        }
        let scale = self.ui_scale();
        // Room for the project's name before the tabs: its letters at this scale.
        let name = self.project.as_ref().map(|p| p.name.as_str());
        let Some(lead) = self.renderer.as_mut().map(|r| r.project_lead(name, scale)) else {
            return;
        };
        let (Some(renderer), Some(window)) = (&self.renderer, &self.window) else {
            return;
        };
        let size = window.inner_size();
        self.layout = self
            .config
            .layout
            .layout(size.width as usize, size.height as usize, scale);
        self.layout.set_lead(lead);
        // The grid gets as many cells as fit in the text area,
        // e.g. 970x670 pixels with 11x24 cells: 88 cols, 27 rows.
        let (cell_w, cell_h) = renderer.cell_size();
        let cols = self.layout.text.w / cell_w;
        let rows = self.layout.text.h / cell_h;

        // All tabs, not only the one on screen, other projects' too: they should
        // already have the right size when you switch to them.
        if rows > 0 && cols > 0 {
            let parked = self.parked.iter_mut().flat_map(|w| w.tabs.iter_mut());
            for tab in self.tabs.iter_mut().chain(parked) {
                tab.resize(rows, cols);
            }
        }
        // The rows were cut again: the matches are somewhere else now.
        self.find(false);
    }

    /// A key was pressed: one of Kel's shortcuts (see `keys::shortcut`),
    /// or a key for the shell.
    fn on_key(&mut self, event: KeyEvent) {
        self.restart_blink();
        self.sidebar_peek = false; // a key in a peek: it stays
        // An open card gets every key: it's typed in.
        if self.card.is_some() {
            self.card_key(&event);
            return;
        }
        // The list of tabs is up: its keys first.
        if self.switcher_up() && self.switcher_key(&event) {
            return;
        }
        let shortcut = keys::shortcut(event.physical_key, self.modifiers);
        if self.help.is_some() {
            self.help_key(&event, shortcut);
            return;
        }
        // Ctrl+Shift+P, Ctrl+Shift+F and Ctrl+Shift+A still work in the palette,
        // the search and the ask box: they close one, or swap one for another.
        let switch = matches!(
            shortcut,
            Some(Action::Palette | Action::Find | Action::Sidebar | Action::Ask)
        );
        if self.sidebar.is_some() && !switch {
            self.sidebar_key(&event);
            return;
        }
        if self.palette.is_some() && !switch {
            self.palette_key(&event);
            return;
        }
        if self.search.is_some() && !switch {
            self.search_key(&event, shortcut);
            return;
        }
        if self.ask.is_some() && !switch {
            self.ask_key(&event, shortcut);
            return;
        }
        if self.start.is_some() && self.start_key(&event, shortcut.as_ref()) {
            return;
        }
        if let Some(action) = shortcut {
            self.run(action);
            return;
        }
        // Any other key: jump back to the bottom, then translate it to bytes for the shell
        let app_cursor = {
            let mut terminal = self.tab().terminal.lock().unwrap();
            terminal.grid.scroll_to_bottom();
            terminal.app_cursor
        };
        let text = event.text.as_deref();
        let key = (&event.logical_key, event.physical_key);
        let bytes = if keys::MAC {
            let option_as_alt = self.config.option_as_alt;
            keys::mac_key_to_bytes(key, text, self.modifiers, option_as_alt)
        } else {
            keys::key_to_bytes(key, text, self.modifiers)
        };
        if let Some(bytes) = bytes {
            let bytes = if app_cursor {
                keys::app_cursor(bytes)
            } else {
                bytes
            };
            self.send_to_shell(&bytes);
        }
    }
}
