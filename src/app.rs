//! The application: owns the window and the tabs, and reacts to every event.
//!
//! The **main thread** runs the winit event loop: keys, mouse, resize, redraw.
//! Each tab has its own shell and two threads of its own (see `tab`), which
//! report back with `UserEvent`s.

use crate::config::{self, Config};
use crate::keys::{self, Action};
use crate::layout::{Layout, WindowButton};
use crate::mouse::{self, Tracking};
use crate::project::{self, Project, TabRecipe};
use crate::renderer::{self, Chrome, Fonts, Renderer, TabLabel};
use crate::tab::{Tab, TabId};
use crate::term::Terminal;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};
use vte::Parser;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ControlFlow;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, ResizeDirection, Window, WindowId};

// Starting size, in cells. After that, the size comes from the window (`on_resize`).
const ROWS: usize = 24;
const COLS: usize = 80;
/// Two clicks closer than this are a double click (the Windows default).
const DOUBLE_CLICK: Duration = Duration::from_millis(500);
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

/// How long "close?" waits for the second press.
const CONFIRM_TIME: Duration = Duration::from_secs(3);

/// What a second press would close.
#[derive(Clone, Copy, PartialEq)]
enum Closing {
    Tab(TabId),
    Window,
}

/// A close waiting for a second press, because it would end a running command.
struct PendingClose {
    what: Closing,
    /// Shown in the tab bar meanwhile.
    message: String,
    until: Instant,
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
    /// The button whose press went to the program (see `on_mouse_button`): its
    /// moves and its release go there too.
    mouse_button: Option<mouse::Button>,
    /// The window has the keyboard focus.
    focused: bool,
    /// The tab last told it has the focus (see `update_focus`).
    focused_tab: Option<TabId>,

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
    /// The window button under the mouse, if any.
    hovered: Option<WindowButton>,
    /// When the empty title bar was last clicked, to notice a double click.
    last_click: Option<Instant>,
    /// The window is closing (see `close_window`): `about_to_wait` ends the event loop.
    quit: bool,
    /// A close that asked "sure?" and waits for the second press.
    pending_close: Option<PendingClose>,
    /// The mouse is at the window's edge: pressing there resizes the window this way.
    resize_edge: Option<ResizeDirection>,
    /// Something went wrong (a config error...), shown in the tab bar until it's fixed.
    status: Option<String>,
    /// The open project, if Kel was started with one (`kel <name>`): its name and
    /// folder. Saved when the window closes (see `save_project`).
    project: Option<(String, PathBuf)>,
}

impl App {
    /// Starts the tabs: the project's, if `project_name` is given, else one.
    /// The window comes later, in `resumed`.
    /// `proxy` is how the tabs' threads wake up the event loop.
    pub fn new(
        proxy: EventLoopProxy<UserEvent>,
        project_name: Option<String>,
    ) -> anyhow::Result<Self> {
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
        // A project that can't be read: say why, and open a plain tab.
        let project = project_name.as_deref().and_then(|name| {
            project::load(name)
                .map_err(|e| problems.push(format!("{e:#}")))
                .ok()
        });
        let recipes = project
            .as_ref()
            .map_or_else(|| vec![TabRecipe::default()], Project::tabs);
        // Without a project: where Kel runs.
        let folder_of = |recipe: &TabRecipe| project.as_ref().map(|p| p.folder_of(recipe));

        // The first tab shows the problems. A shell that doesn't start is one more,
        // and all tabs get the default shell instead.
        let mut shell = config.shell.clone();
        let folder = folder_of(&recipes[0]);
        let first = match Tab::spawn(
            0,
            &shell,
            folder.as_deref(),
            screen(&problems),
            proxy.clone(),
        ) {
            Ok(tab) => tab,
            Err(e) => {
                problems.push(format!("can't start shell \"{shell}\": {e:#}"));
                shell = Config::default().shell;
                Tab::spawn(
                    0,
                    &shell,
                    folder.as_deref(),
                    screen(&problems),
                    proxy.clone(),
                )?
            }
        };
        let mut tabs = vec![first];
        let mut status = None;
        for (id, recipe) in (1..).zip(&recipes[1..]) {
            let folder = folder_of(recipe);
            let screen = Terminal::new(ROWS, COLS);
            match Tab::spawn(id, &shell, folder.as_deref(), screen, proxy.clone()) {
                Ok(tab) => tabs.push(tab),
                Err(e) => status = Some(format!("can't open a tab: {e:#}")),
            }
        }
        // Each tab's name and command, by its id: a tab that failed has no place.
        for tab in &mut tabs {
            let recipe = &recipes[tab.id as usize];
            tab.name = recipe.name.clone();
            if let Some(command) = &recipe.command {
                tab.type_at_start(command, &shell);
            }
        }
        let next_id = recipes.len() as TabId;
        // No window yet: the real layout comes with the first `on_resize`.
        let layout = config.layout.layout(0, 0, 1.0);

        Ok(App {
            window: None,
            renderer: None,
            tabs,
            active: 0,
            next_id,
            proxy,
            modifiers: ModifiersState::empty(),

            mouse_pos: (0, 0),
            mouse_cell: (0, 0),
            selection: None,
            selecting: false,
            mouse_button: None,
            focused: false,
            focused_tab: None,

            cursor_on: true,
            next_blink: Instant::now() + BLINK_INTERVAL,

            scroll_rest: 0.0,

            config,
            config_modified: config::modified(),
            fonts: Some(fonts),
            layout,
            hovered: None,
            last_click: None,
            quit: false,
            pending_close: None,
            resize_edge: None,
            status,
            project: project_name.zip(project.map(|project| project.folder)),
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

    /// Opens a new tab at the end, the same size as the others and in the current
    /// tab's folder, and shows it.
    fn new_tab(&mut self) {
        let (rows, cols) = {
            let terminal = self.tab().terminal.lock().unwrap();
            (terminal.grid.rows(), terminal.grid.cols())
        };
        let id = self.next_id;
        self.next_id += 1;
        let screen = Terminal::new(rows, cols);
        let cwd = self.tab().cwd();
        match Tab::spawn(
            id,
            &self.config.shell,
            cwd.as_deref(),
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
        // Seen now.
        self.tabs[index].activity = false;
        self.tabs[index].command.done = None;
        self.update_window_title();
        // The selection and the half-scrolled line belonged to the other tab.
        self.selection = None;
        self.scroll_rest = 0.0;
        self.update_focus();
        self.request_redraw();
    }

    /// Tells the programs that asked (`ESC[?1004h`) when they get the focus
    /// (`ESC[I`) and lose it (`ESC[O`). nvim does, e.g. to reload a file changed
    /// meanwhile. A tab has the focus while it's on screen and the window has it.
    fn update_focus(&mut self) {
        let now = self.focused.then(|| self.tab().id);
        if now == self.focused_tab {
            return;
        }
        let before = std::mem::replace(&mut self.focused_tab, now);
        for (id, bytes) in [(before, b"\x1b[O"), (now, b"\x1b[I")] {
            // `before` may be gone: closed.
            if let Some(tab) = self.tabs.iter_mut().find(|tab| Some(tab.id) == id) {
                let asked = tab.terminal.lock().unwrap().focus_events;
                if asked {
                    tab.write(bytes);
                }
            }
        }
    }

    /// Ctrl+Shift+W: ends the shell of the tab on screen. If a command runs
    /// (nvim, `cargo run`), only on the second press.
    fn close_tab(&mut self) {
        let tab = self.tab();
        let (id, running) = (tab.id, tab.command.running());
        let message = "a command runs here: Ctrl+Shift+W again to close";
        if self.confirm_close(Closing::Tab(id), running, message.to_string()) {
            // Only ends the shell; the tab goes when the shell is gone (`remove_tab`),
            // the same way as after `exit`.
            self.tab_mut().kill();
        }
    }

    /// The close button or Alt+F4: saves the project and closes Kel. If a command
    /// runs in any tab, or the project can't be saved, only on the second time.
    fn close_window(&mut self) {
        // Saved at every try: a try that only asks writes the same file again later.
        let saved = self.save_project();
        let running = self.tabs.iter().filter(|tab| tab.command.running()).count();
        let question = match (saved, running) {
            (Err(e), _) => Some(format!("{e:#}: close again to quit")),
            (Ok(()), 0) => None,
            (Ok(()), 1) => Some("a command runs in a tab: close again to quit".to_string()),
            (Ok(()), n) => Some(format!("commands run in {n} tabs: close again to quit")),
        };
        let ask = question.is_some();
        if self.confirm_close(Closing::Window, ask, question.unwrap_or_default()) {
            self.quit = true;
        }
    }

    /// Writes the open project's file: its tabs as they are now, each in its last
    /// folder. Only what you wrote in the file comes back, the name and the command:
    /// Kel never adds a command, so opening a project never runs `git push` by itself.
    /// A tab opened since (Ctrl+Shift+T) is saved as a plain one, with its folder.
    ///
    /// Only when the window closes: after `exit` in each tab, the file stays as it
    /// was, not a project of no tabs.
    fn save_project(&self) -> anyhow::Result<()> {
        let Some((name, folder)) = &self.project else {
            return Ok(()); // no project, or one that couldn't be read: not written over
        };
        let mut project = Project {
            folder: folder.clone(),
            tabs: Vec::new(),
        };
        let tabs = self.tabs.iter().map(|tab| TabRecipe {
            name: tab.name.clone(),
            folder: tab.cwd().and_then(|cwd| project.tab_folder(&cwd)),
            command: tab.start_command.clone(),
        });
        project.tabs = tabs.collect();
        project::save(name, &project)
    }

    /// Returns `true` if `what` may close now: nothing would be lost (`!running`),
    /// or it's the second press within `CONFIRM_TIME`. Else asks, with `message`.
    fn confirm_close(&mut self, what: Closing, running: bool, message: String) -> bool {
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

    /// The window's own title, seen in the taskbar and in Alt+Tab: the tab on screen.
    fn update_window_title(&self) {
        if let Some(window) = &self.window {
            let title = match &self.project {
                Some((project, _)) => format!("{} - {project} - Kel", self.tab().label()),
                None => format!("{} - Kel", self.tab().label()),
            };
            window.set_title(&title);
        }
    }

    /// The tab after (`step` = 1) or before (-1) the one on screen,
    /// going round from the last to the first.
    fn next_tab(&mut self, step: isize) {
        let count = self.tabs.len() as isize;
        let index = (self.active as isize + step).rem_euclid(count);
        self.show_tab(index as usize);
    }

    /// Shows a problem in the tab bar, or (`None`) clears it.
    fn set_status(&mut self, status: Option<String>) {
        if self.status != status {
            self.status = status;
            self.request_redraw();
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

    /// The UI's scale: Windows' display scale times the config's `ui_scale`,
    /// e.g. 1.5 (150%) x 1 = 1.5. Kept between a quarter and 4 times the size.
    fn ui_scale(&self) -> f32 {
        let display = self
            .window
            .as_ref()
            .map_or(1.0, |w| w.scale_factor() as f32);
        (display * self.config.ui_scale).clamp(0.25, 4.0)
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

        // `Some`: something went wrong, and the tab bar says what.
        let error = match config::load() {
            Ok(new) => {
                let mut error = None;
                // Only what changed, so saving another setting doesn't undo your zoom.
                if new.font != self.config.font {
                    match renderer::load_fonts(&new.font) {
                        Ok(fonts) => self.set_fonts(fonts),
                        Err(e) => error = Some(format!("{e:#}")), // keeps the old font
                    }
                }
                if new.font_size != self.config.font_size {
                    self.set_font_size(new.font_size);
                }
                if new.layout != self.config.layout || new.ui_scale != self.config.ui_scale {
                    self.config.layout = new.layout.clone();
                    self.config.ui_scale = new.ui_scale;
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
            // e.g. "config.toml, line 3: invalid type: string "big", expected f32"
            Err(e) => Some(format!("{e:#}")),
        };
        self.set_status(error);
    }

    // -----------------------------------------------------------------------
    // Shortcuts: what each `Action` does.
    // -----------------------------------------------------------------------

    /// Does what a shortcut stands for. Which key is which action: `keys::shortcut`.
    fn run(&mut self, action: Action) {
        match action {
            Action::NewTab => self.new_tab(),
            Action::CloseTab => self.close_tab(),
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
        // No Windows title bar: Kel draws its own (the tabs, the buttons).
        let attrs = Window::default_attributes()
            .with_title("Kel") // still shown in the taskbar
            .with_decorations(false);
        #[cfg(target_os = "windows")]
        let attrs = {
            use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
            // Without the title bar, Windows also drops the window's shadow: ask for it
            // back. And round corners, like other windows on Windows 11.
            attrs
                .with_undecorated_shadow(true)
                .with_corner_preference(CornerPreference::Round)
        };
        let window = Rc::new(event_loop.create_window(attrs).unwrap());
        // The cell size comes from the font, which the renderer holds, and the renderer
        // needs the window. So: window first, then renderer, then the size for
        // 80x24 cells plus the spaces around them.
        let theme = self.config.colors.clone();
        let text_scale = window.scale_factor() as f32;
        let font_size = self.config.font_size;
        let renderer = Renderer::new(window.clone(), fonts, font_size, text_scale, theme).unwrap();
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
                let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) else {
                    return;
                };
                // The output can hold a new title (`ESC]0;...`), for any tab.
                let renamed = tab.refresh_title();
                // A project's command waits for the shell's first prompt.
                tab.type_startup_when_ready();
                // And command marks (`ESC]133;...`).
                let marked = tab.refresh_command(on_screen);
                let lit = !on_screen && tab.mark_activity();
                if on_screen && renamed {
                    self.update_window_title();
                }
                if on_screen || renamed || marked || lit {
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
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.on_key(event)
            }
            WindowEvent::MouseWheel { delta, .. } => self.on_mouse_wheel(delta),
            WindowEvent::CursorMoved { position, .. } => self.on_mouse_move(position),
            WindowEvent::CursorLeft { .. } => self.set_hovered(None),
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse_button(state, button),
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                self.update_focus();
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
        if Instant::now() >= self.next_blink {
            self.cursor_on = !self.cursor_on;
            self.next_blink = Instant::now() + BLINK_INTERVAL;
            self.request_redraw();
            // The same timer looks for a saved config file, twice a second,
            // and lets an unanswered "close?" go.
            self.reload_config_if_changed();
            if self
                .pending_close
                .as_ref()
                .is_some_and(|pending| Instant::now() >= pending.until)
            {
                self.pending_close = None;
            }
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
    /// The blink timer redraws twice a second, so a ring for a busy tab shows up
    /// at most half a second after `BUSY_DELAY`, without a timer of its own.
    fn on_redraw(&mut self) {
        let labels: Vec<TabLabel> = self
            .tabs
            .iter()
            .map(|tab| TabLabel {
                title: tab.label(),
                icon: tab.icon,
                activity: tab.activity,
                busy: tab.command.busy(),
                done: tab.command.done,
            })
            .collect();
        let chrome = Chrome {
            tabs: &labels,
            active: self.active,
            hovered: self.hovered,
            maximized: self.window.as_ref().is_some_and(|w| w.is_maximized()),
            status: self.status.as_deref(),
            question: self.pending_close.as_ref().map(|p| p.message.as_str()),
        };
        if let Some(renderer) = &mut self.renderer {
            renderer.draw(
                &self.tabs[self.active].terminal.lock().unwrap().grid,
                &self.layout,
                &chrome,
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
        let scale = self.ui_scale();
        self.layout = self
            .config
            .layout
            .layout(size.width as usize, size.height as usize, scale);
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
        let key = (&event.logical_key, event.physical_key);
        if let Some(bytes) = keys::key_to_bytes(key, text, self.modifiers) {
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
        let moved = (row, col) != self.mouse_cell;
        self.mouse_pos = (x, y);
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
        let button = if edge.is_some() {
            None
        } else {
            self.layout.button_at(x, y)
        };
        self.set_hovered(button);

        if self.selecting {
            if let Some(selection) = &mut self.selection {
                selection.end = (row, col);
            }
            self.request_redraw();
        } else if moved
            && (self.mouse_button.is_some() || self.over_text() && self.program_takes_mouse())
        {
            // Dragging in the program, or just moving if it asked for every move.
            self.report_mouse(mouse::Event::Move(self.mouse_button));
        }
    }

    /// The program on screen asked for the mouse (nvim does), and Shift isn't held:
    /// Shift + mouse is always Kel's, to select text the program shows.
    fn program_takes_mouse(&self) -> bool {
        !self.modifiers.shift_key() && self.tab().terminal.lock().unwrap().mouse != Tracking::Off
    }

    /// The mouse is over the terminal panel: not the title bar, not an edge.
    fn over_text(&self) -> bool {
        let (x, y) = self.mouse_pos;
        self.resize_edge.is_none() && !self.layout.title_bar.contains(x, y)
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
    fn on_mouse_button(&mut self, state: ElementState, button: MouseButton) {
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

    /// The window button under the mouse changed: draw the buttons again.
    fn set_hovered(&mut self, button: Option<WindowButton>) {
        if self.hovered != button {
            self.hovered = button;
            self.request_redraw();
        }
    }

    /// A click in the title bar: a window button, a tab, or the empty bar, which
    /// moves the window (and maximizes it on a double click), like a Windows title bar.
    fn on_title_bar_click(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let (x, y) = self.mouse_pos;
        if let Some(button) = self.layout.button_at(x, y) {
            match button {
                WindowButton::Minimize => window.set_minimized(true),
                WindowButton::Maximize => window.set_maximized(!window.is_maximized()),
                WindowButton::Close => self.close_window(),
            }
            return;
        }
        let tabs = self.layout.tabs(self.tabs.len());
        if let Some(index) = tabs.iter().position(|tab| tab.contains(x, y)) {
            self.show_tab(index);
            return;
        }
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

    /// Left mouse button: pressing starts a new selection, releasing finishes it.
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
            ElementState::Pressed
                if self
                    .layout
                    .title_bar
                    .contains(self.mouse_pos.0, self.mouse_pos.1) =>
            {
                self.on_title_bar_click()
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
