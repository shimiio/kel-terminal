//! The application: owns the window and the tabs, and reacts to every event.
//!
//! The **main thread** runs the winit event loop: keys, mouse, resize, redraw.
//! Each tab has its own shell and two threads of its own (see `tab`), which
//! report back with `UserEvent`s.

use crate::config::{self, Config};
use crate::keys::{self, Action};
use crate::layout::Layout;
use crate::renderer::{self, Fonts, Renderer};
use crate::tab::{Tab, TabId};
use crate::term::Terminal;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};
use vte::Parser;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ControlFlow;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

// Starting size, in cells. After that, the size comes from the window (`on_resize`).
const ROWS: usize = 24;
const COLS: usize = 80;
/// How long the cursor stays on, then off.
const BLINK_INTERVAL: Duration = Duration::from_millis(500);

/// The selected area, from where the mouse was pressed to where it is now. (row, col)
#[derive(Clone, Copy, Debug)]
pub struct Selection {
    pub start: (usize, usize),
    pub end: (usize, usize),
}

impl Selection {
    /// Is the cell at (row, col) inside the selection, in reading order?
    /// Tuples compare row first, then col: (2, 70) < (3, 5), because row 2 comes before row 3.
    pub fn contains(&self, row: usize, col: usize) -> bool {
        let (first, last) = self.ordered();
        first <= (row, col) && (row, col) <= last
    }

    /// Start and end in reading order (the earlier one first).
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}

/// Custom events sent from the tabs' threads to wake up the event loop.
/// Each one says which tab it's about.
#[derive(Debug)]
pub enum UserEvent {
    /// The reader thread parsed new shell output into the grid.
    PtyOutput(TabId),
    /// The program asked the terminal something; these bytes are the answer.
    Reply(TabId, Vec<u8>),
    /// The shell ended.
    PtyExited(TabId),
}

pub struct App {
    // Created in `resumed`, so they are `None` until the window exists.
    window: Option<Rc<Window>>,
    renderer: Option<Renderer>,
    /// Never empty: closing the last tab closes Kel.
    tabs: Vec<Tab>,
    /// The tab on screen: its place in `tabs`.
    active: usize,
    /// The id the next new tab gets.
    next_id: TabId,
    /// Given to each new tab, so its threads can wake up the event loop.
    proxy: EventLoopProxy<UserEvent>,
    /// Which of Shift/Ctrl/Alt are held right now (updated by `ModifiersChanged`).
    modifiers: ModifiersState,

    /// Where the mouse pointer is, in pixels, and the cell under it as (row, col).
    mouse_pos: (usize, usize),
    mouse_cell: (usize, usize),
    /// The mouse selection, if any. Screen positions, so it doesn't follow scrolling.
    selection: Option<Selection>,
    /// True while the left button is held down (dragging).
    selecting: bool,

    /// Is the blinking cursor in its "on" half right now?
    cursor_on: bool,
    /// When the cursor switches on/off next.
    next_blink: Instant,

    /// Scrolling that doesn't add up to a whole line yet (touchpads send tiny steps).
    scroll_rest: f32,

    config: Config,
    /// When the config file was saved, as of the last load (see `reload_config_if_changed`).
    config_modified: Option<SystemTime>,
    /// Loaded in `new`, so a font problem can be shown with the others;
    /// handed to the renderer in `resumed`.
    fonts: Option<Fonts>,
    /// Where the panel and the text are in the window. Updated in `on_resize`.
    layout: Layout,
}

impl App {
    /// Starts the first tab. The window comes later, in `resumed`.
    /// `proxy` is how the tabs' threads wake up the event loop.
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> anyhow::Result<Self> {
        // A broken config or a missing shell shouldn't stop Kel: start with the
        // defaults and show what went wrong at the top of the screen.
        let mut problems = Vec::new();
        let config = config::load().unwrap_or_else(|e| {
            problems.push(format!("{e:#}"));
            Config::default()
        });
        let fonts = match renderer::load_fonts(&config.font) {
            Ok(fonts) => fonts,
            Err(e) => {
                problems.push(format!("{e:#}"));
                renderer::load_fonts(&Config::default().font)?
            }
        };
        // A screen with the problems so far in red. The shell starts below them:
        // when ConPTY asks where the cursor is, the answer is the line after them.
        let screen = |problems: &[String]| {
            let mut terminal = Terminal::new(ROWS, COLS);
            for problem in problems {
                let text = format!("\x1b[31mkel: {problem}\x1b[m\n\n").replace('\n', "\r\n");
                Parser::new().advance(&mut terminal, text.as_bytes());
            }
            terminal
        };
        let first = match Tab::spawn(0, &config.shell, screen(&problems), proxy.clone()) {
            Ok(tab) => tab,
            Err(e) => {
                problems.push(format!("can't start shell \"{}\": {e:#}", config.shell));
                Tab::spawn(
                    0,
                    &Config::default().shell,
                    screen(&problems),
                    proxy.clone(),
                )?
            }
        };

        Ok(App {
            window: None,
            renderer: None,
            tabs: vec![first],
            active: 0,
            next_id: 1,
            proxy,
            modifiers: ModifiersState::empty(),

            mouse_pos: (0, 0),
            mouse_cell: (0, 0),
            selection: None,
            selecting: false,

            cursor_on: true,
            next_blink: Instant::now() + BLINK_INTERVAL,

            scroll_rest: 0.0,

            config,
            config_modified: config::modified(),
            fonts: Some(fonts),
            layout: Layout::default(),
        })
    }

    // -----------------------------------------------------------------------
    // Tabs
    // -----------------------------------------------------------------------

    /// The tab on screen.
    fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    /// Sends bytes to the shell of the tab on screen, as if they were typed.
    fn send_to_shell(&mut self, bytes: &[u8]) {
        // Anything sent to the shell can change the screen, so an old mouse
        // selection would point at the wrong text: remove it.
        if self.selection.take().is_some() {
            self.request_redraw();
        }
        self.tab_mut().write(bytes);
    }

    /// Opens a new tab at the end, the same size as the others, and shows it.
    fn new_tab(&mut self) {
        let (rows, cols) = {
            let terminal = self.tab().terminal.lock().unwrap();
            (terminal.grid.rows(), terminal.grid.cols())
        };
        let id = self.next_id;
        self.next_id += 1;
        let screen = Terminal::new(rows, cols);
        match Tab::spawn(id, &self.config.shell, screen, self.proxy.clone()) {
            Ok(tab) => {
                self.tabs.push(tab);
                self.show_tab(self.tabs.len() - 1);
            }
            Err(e) => self.set_title(&format!("Kel - can't open a tab: {e:#}")),
        }
    }

    /// The shell of tab `id` ended (`exit`, or Ctrl+Shift+W): the tab goes away.
    /// The last one closes Kel.
    fn remove_tab(&mut self, id: TabId, event_loop: &ActiveEventLoop) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return; // already gone
        };
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            event_loop.exit();
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
    fn show_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.active = index;
        // The selection and the half-scrolled line belonged to the other tab.
        self.selection = None;
        self.scroll_rest = 0.0;
        self.request_redraw();
    }

    /// The tab after (`step` = 1) or before (-1) the one on screen,
    /// going round from the last to the first.
    fn next_tab(&mut self, step: isize) {
        let count = self.tabs.len() as isize;
        let index = (self.active as isize + step).rem_euclid(count);
        self.show_tab(index as usize);
    }

    fn set_title(&self, title: &str) {
        if let Some(window) = &self.window {
            window.set_title(title);
        }
    }

    // -----------------------------------------------------------------------
    // Font, config, title
    // -----------------------------------------------------------------------

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

    /// After the cell got a new size: fit the grid to the window again.
    fn cell_size_changed(&mut self) {
        // The selection points at cells that move around when the grid changes size.
        self.selection = None;
        self.on_resize();
        self.request_redraw();
    }

    /// Loads the config file again if it was saved since the last look, and applies it.
    /// A broken file keeps the old settings and shows the error in the window title.
    fn reload_config_if_changed(&mut self) {
        let modified = config::modified();
        if modified == self.config_modified {
            return;
        }
        self.config_modified = modified;

        // `Some`: something went wrong, and the title says what.
        let error = match config::load() {
            Ok(new) => {
                let mut error = None;
                // Only what changed, so saving another setting doesn't undo your zoom.
                if new.font != self.config.font {
                    match renderer::load_fonts(&new.font) {
                        Ok(fonts) => self.set_fonts(fonts),
                        Err(e) => error = Some(format!("Kel - {e:#}")), // keeps the old font
                    }
                }
                if new.font_size != self.config.font_size {
                    self.set_font_size(new.font_size);
                }
                if new.layout != self.config.layout {
                    self.config.layout = new.layout.clone();
                    self.cell_size_changed(); // not the cell, but the room for cells
                }
                if new.colors != self.config.colors
                    && let Some(renderer) = &mut self.renderer
                {
                    renderer.set_theme(new.colors.clone());
                    self.request_redraw();
                }
                self.config = new;
                error
            }
            // The error has several lines: the place ("...config.toml: TOML parse error
            // at line 1, column 13"), a drawing of that line, and the reason
            // ("invalid type: string "big", expected f32"). A title fits one line,
            // so it gets the first and the last.
            Err(e) => {
                let error = format!("{e:#}");
                let mut lines = error.lines();
                let first = lines.next().unwrap_or("");
                let last = lines.last().unwrap_or("");
                Some(format!("Kel - config error: {first} {last}"))
            }
        };
        match error {
            Some(error) => self.set_title(&error),
            None => self.set_title("Kel"),
        }
    }

    // -----------------------------------------------------------------------
    // Shortcuts: what each `Action` does.
    // -----------------------------------------------------------------------

    /// Does what a shortcut stands for. Which key is which action: `keys::shortcut`.
    fn run(&mut self, action: Action) {
        match action {
            Action::NewTab => self.new_tab(),
            // Only ends the shell; the tab goes when the shell is gone (`remove_tab`),
            // the same way as after `exit`.
            Action::CloseTab => self.tab_mut().kill(),
            Action::NextTab => self.next_tab(1),
            Action::PreviousTab => self.next_tab(-1),
            Action::GoToTab(n) => self.show_tab(n - 1), // tab 1 is at index 0
            Action::LastTab => self.show_tab(self.tabs.len() - 1),
            Action::Paste => self.paste(),
            Action::Copy => self.copy(),
            Action::Interrupt => self.send_to_shell(&[0x03]), // the byte Ctrl+C stands for
            Action::ZoomIn => self.zoom(1.0),
            Action::ZoomOut => self.zoom(-1.0),
            Action::ZoomReset => self.set_font_size(self.config.font_size),
            Action::OpenConfig => {
                if let Ok(path) = config::path() {
                    // Explorer opens a file the same way as a double-click on it.
                    let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
                }
            }
        }
    }

    /// Pastes the clipboard into the shell.
    fn paste(&mut self) {
        if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            let bracketed = self.tab().terminal.lock().unwrap().bracketed_paste;
            self.send_to_shell(&keys::paste_bytes(&text, bracketed));
        }
    }

    /// Copies the mouse selection. Without one, it's a Ctrl+C for the shell,
    /// and PowerShell then copies its own keyboard selection.
    fn copy(&mut self) {
        let Some(selection) = self.selection else {
            self.send_to_shell(&[0x03]);
            return;
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
        self.selection = None;
        self.request_redraw();
    }

    /// Font one pixel bigger (`step` = 1) or smaller (-1).
    fn zoom(&mut self, step: f32) {
        if let Some(renderer) = &self.renderer {
            self.set_font_size(renderer.font_size() + step);
        }
    }

    /// Shows the cursor and starts the blink over, so it doesn't vanish while you type.
    fn restart_blink(&mut self) {
        self.cursor_on = true;
        self.next_blink = Instant::now() + BLINK_INTERVAL;
        self.request_redraw();
    }
}

// winit calls these methods when something happens; we never call them ourselves.
impl ApplicationHandler<UserEvent> for App {
    /// Called when the app starts. A window can only be created from here.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(fonts) = self.fonts.take() else {
            return; // already started
        };
        let attrs = Window::default_attributes().with_title("Kel");
        let window = Rc::new(event_loop.create_window(attrs).unwrap());
        // The cell size comes from the font, which the renderer holds, and the renderer
        // needs the window. So: window first, then renderer, then the size for
        // 80x24 cells plus the spaces around them.
        let theme = self.config.colors.clone();
        let renderer = Renderer::new(window.clone(), fonts, self.config.font_size, theme).unwrap();
        let (cell_w, cell_h) = renderer.cell_size();
        let (w, h) = self.config.layout.window_size(COLS * cell_w, ROWS * cell_h);
        let _ = window.request_inner_size(PhysicalSize::new(w as u32, h as u32));
        self.renderer = Some(renderer);
        self.window = Some(window);
        self.on_resize();
    }

    /// Our own events, sent by the tabs' threads.
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            // Only the tab on screen needs a redraw. The others keep their new
            // output in their grids, ready for when you switch to them.
            UserEvent::PtyOutput(id) => {
                if id == self.tab().id {
                    self.request_redraw();
                }
            }
            // Not `send_to_shell`: an answer isn't typing, so the mouse selection stays.
            UserEvent::Reply(id, bytes) => {
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                    tab.write(&bytes);
                }
            }
            UserEvent::PtyExited(id) => self.remove_tab(id, event_loop),
        }
    }

    /// Every window event arrives here and is passed to its own handler below.
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.on_redraw(),
            WindowEvent::Resized(_) => self.on_resize(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.on_key(event)
            }
            WindowEvent::MouseWheel { delta, .. } => self.on_mouse_wheel(delta),
            WindowEvent::CursorMoved { position, .. } => self.on_mouse_move(position),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => self.on_left_button(state),
            _ => {}
        }
    }

    /// Called when winit has handled all events and is about to sleep.
    /// We use it as the timer for the blinking cursor.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if Instant::now() >= self.next_blink {
            self.cursor_on = !self.cursor_on;
            self.next_blink = Instant::now() + BLINK_INTERVAL;
            self.request_redraw();
            // The same timer looks for a saved config file, twice a second.
            self.reload_config_if_changed();
        }
        // Sleep until the next event, or until it's time to blink, whichever comes first.
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_blink));
    }
}

// ---------------------------------------------------------------------------
// Event handlers, one per kind of event.
// ---------------------------------------------------------------------------
impl App {
    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Draws the grid (and the mouse selection) into the window.
    fn on_redraw(&mut self) {
        let titles: Vec<String> = self.tabs.iter().map(|tab| tab.title.clone()).collect();
        if let Some(renderer) = &mut self.renderer {
            renderer.draw(
                &self.tabs[self.active].terminal.lock().unwrap().grid,
                &self.layout,
                &titles,
                self.active,
                self.selection,
                self.cursor_on,
            );
        }
    }

    /// The window changed size: resize the grid and tell the shell.
    /// Uses the window's *current* size, because the event's size can be outdated.
    fn on_resize(&mut self) {
        let (Some(renderer), Some(window)) = (&self.renderer, &self.window) else {
            return;
        };
        let size = window.inner_size();
        self.layout = self
            .config
            .layout
            .layout(size.width as usize, size.height as usize);
        // The grid gets as many cells as fit in the text area,
        // e.g. 970x670 pixels with 11x24 cells: 88 cols, 27 rows.
        let (cell_w, cell_h) = renderer.cell_size();
        let cols = self.layout.text.w / cell_w;
        let rows = self.layout.text.h / cell_h;

        // All tabs, not only the one on screen: the others should already have
        // the right size when you switch to them.
        if rows > 0 && cols > 0 {
            for tab in &mut self.tabs {
                tab.resize(rows, cols);
            }
        }
    }

    /// A key was pressed: one of Kel's shortcuts (see `keys::shortcut`),
    /// or a key for the shell.
    fn on_key(&mut self, event: KeyEvent) {
        self.restart_blink();
        if let Some(action) = keys::shortcut(event.physical_key, self.modifiers) {
            self.run(action);
            return;
        }
        // Any other key: jump back to the bottom, then translate it to bytes for the shell
        self.tab().terminal.lock().unwrap().grid.scroll_to_bottom();
        let text = event.text.as_deref();
        if let Some(bytes) = keys::key_to_bytes(&event.logical_key, text, self.modifiers) {
            self.send_to_shell(&bytes);
        }
    }

    /// Mouse wheel or touchpad: scroll the view into the scrollback.
    fn on_mouse_wheel(&mut self, delta: MouseScrollDelta) {
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
            self.request_redraw();
        }
    }

    /// The mouse moved: remember which cell it's over, and extend the selection while dragging.
    fn on_mouse_move(&mut self, position: PhysicalPosition<f64>) {
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
        self.mouse_pos = (x, y);
        self.mouse_cell = (row, col);

        if self.selecting {
            if let Some(selection) = &mut self.selection {
                selection.end = (row, col);
            }
            self.request_redraw();
        }
    }

    /// Left mouse button: pressing starts a new selection, releasing finishes it.
    fn on_left_button(&mut self, state: ElementState) {
        match state {
            // In the tab bar: a click on a tab shows it. There's no text to select there.
            ElementState::Pressed
                if self
                    .layout
                    .tab_bar
                    .contains(self.mouse_pos.0, self.mouse_pos.1) =>
            {
                let (x, y) = self.mouse_pos;
                let tabs = self.layout.tabs(self.tabs.len());
                if let Some(index) = tabs.iter().position(|tab| tab.contains(x, y)) {
                    self.show_tab(index);
                }
            }
            ElementState::Pressed => {
                self.selection = Some(Selection {
                    start: self.mouse_cell,
                    end: self.mouse_cell,
                });
                self.selecting = true;
                self.request_redraw();
            }
            ElementState::Released => {
                self.selecting = false;
                // Just a click, no drag: no selection.
                if self.selection.is_some_and(|s| s.start == s.end) {
                    self.selection = None;
                    self.request_redraw();
                }
            }
        }
    }
}
