//! The application: owns the window and the tabs, and reacts to every event.
//!
//! The **main thread** runs the winit event loop: keys, mouse, resize, redraw.
//! Each tab has its own shell and two threads of its own (see `tab`), which
//! report back with `UserEvent`s.

use crate::card::{self, Card, Target};
use crate::config::{self, Config};
use crate::font::{self, Fonts};
use crate::glass;
use crate::help::Help;
use crate::icons;
use crate::keys::{self, Action};
use crate::layout::{Layout, Rect, TitleHit, WindowButton};
use crate::links;
use crate::logo;
use crate::mouse::{self, Tracking};
use crate::palette::{Item, Palette};
use crate::project::{self, Project, TabRecipe};
use crate::renderer::{Chrome, Renderer, TabLabel};
use crate::search::Search;
use crate::setup;
use crate::sidebar::{Live, Row, Sidebar};
use crate::start::{self, Start};
use crate::tab::{Tab, TabId};
use crate::term::{Link, Terminal};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};
use vte::Parser;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ControlFlow;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{CursorIcon, Icon, ResizeDirection, UserAttentionType, Window, WindowId};

// Starting size, in cells. After that, the size comes from the window (`on_resize`).
const ROWS: usize = 24;
const COLS: usize = 80;
/// Two clicks closer than this are a double click (the Windows default).
const DOUBLE_CLICK: Duration = Duration::from_millis(500);
/// How long the cursor stays on, then off.
const BLINK_INTERVAL: Duration = Duration::from_millis(500);
/// How long the mouse rests on the sidebar button before the sidebar opens by itself: long
/// enough that passing over it on the way to a tab doesn't.
const PEEK_DELAY: Duration = Duration::from_millis(400);

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

/// How long the terminal flashes when the bell rings (`bell = "flash"`).
const FLASH_TIME: Duration = Duration::from_millis(120);

/// How long "close?" waits for the second press.
const CONFIRM_TIME: Duration = Duration::from_secs(3);

/// What a second press would close.
#[derive(Clone, PartialEq)]
enum Closing {
    Tab(TabId),
    /// A project: the one whose first tab this is.
    Project(TabId),
    /// Deleting the project of this name (open or not).
    Delete(String),
    Window,
    /// Not a close: pasting this text of several lines, which would run them
    /// (see `paste`). The text itself: a new copy asks again.
    Paste(String),
}

/// A close waiting for a second press, because it would end a running command.
struct PendingClose {
    what: Closing,
    /// Shown in the tab bar meanwhile.
    message: String,
    until: Instant,
}

/// How far (pixels at 100%) a pressed tab must move before it's dragged, not
/// clicked: a hand isn't still.
const TAB_DRAG_START: usize = 6;

/// A project held by the mouse in the sidebar: a click opens it on release, a
/// drag (past `TAB_DRAG_START`) moves it up or down.
struct SidebarDrag {
    /// Where the press was, in pixels from the window's top.
    from_y: usize,
    moving: bool,
}

/// A tab held by the mouse (see `drag_tab`).
struct TabDrag {
    /// Where the press was, in pixels from the window's left edge.
    from_x: usize,
    /// It went past `TAB_DRAG_START`: it follows the mouse now.
    moving: bool,
}

/// A project open in this window but not on screen, or home (Kel without a
/// project). Its tabs live on: their shells and commands run, their grids take
/// the output, as a tab does while another one is on screen.
struct Workspace {
    /// `None`: home.
    project: Option<OpenProject>,
    tabs: Vec<Tab>,
    /// The tab that was on screen, shown again on the way back.
    active: usize,
}

impl Workspace {
    /// Its project's name; `None`: home.
    fn name(&self) -> Option<&str> {
        self.project.as_ref().map(|p| p.name.as_str())
    }
}

/// The project on screen (`kel <name>`, or chosen in the sidebar): what's kept
/// of its file.
struct OpenProject {
    name: String,
    folder: PathBuf,
    /// Its `color`: the name before the tabs and the icons. `None`: none, gray
    /// (see `Theme::accent_for`).
    color: Option<u32>,
    /// Its `shell`, for its new tabs. `None`: the config's.
    shell: Option<String>,
}

pub struct App {
    // Created in `resumed`, so they are `None` until the window exists.
    window: Option<Rc<Window>>,
    renderer: Option<Renderer>,
    /// Never empty: the last tab closed, another workspace comes, or Kel closes
    /// (see `leave_empty`).
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
    /// The mouse is over the window (else `mouse_pos` is where it left).
    mouse_inside: bool,
    mouse_cell: (usize, usize),
    /// The mouse selection, if any. Screen positions, so it doesn't follow scrolling.
    selection: Option<Selection>,
    /// While the left button is held: the cell it was pressed on. A selection
    /// starts only once the mouse leaves that cell: a click selects nothing.
    selecting: Option<(usize, usize)>,
    /// The button whose press went to the program (see `on_mouse_button`): its
    /// moves and its release go there too.
    mouse_button: Option<mouse::Button>,
    /// The window has the keyboard focus.
    focused: bool,
    /// The tab last told it has the focus (see `update_focus`).
    focused_tab: Option<TabId>,

    /// Is the blinking cursor in its "on" half right now? Always, without blinking
    /// (`cursor_blink = false`).
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
    hovered: Option<TitleHit>,
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
    /// The project on screen (`kel <name>`, or one chosen in the sidebar); `None`:
    /// home. Saved when you leave it and when the window closes (`save_project`).
    project: Option<OpenProject>,
    /// The other projects open in this window, home among them if you left it:
    /// out of sight, still running (see `switch_to`). The last one is the one you
    /// were in last. `tabs`, `active` and `project` are always the one on screen,
    /// so everything else in `App` only knows about those.
    parked: Vec<Workspace>,
    /// The command palette, while it's open. It gets the keys then, not the shell.
    palette: Option<Palette>,
    /// The search in the output, while it's open. It gets the keys then too.
    search: Option<Search>,
    /// The link under the mouse while Ctrl is held (see `update_link`): where it
    /// is and its text. Underlined, and Ctrl+click opens it.
    link: Option<Link>,
    /// The tab under the pressed mouse button, until it's let go.
    tab_drag: Option<TabDrag>,
    /// The sidebar of projects, while it's open. It gets the keys and the clicks.
    sidebar: Option<Sidebar>,
    /// Home's start screen, until a key or a click puts it away (see `start`).
    start: Option<Start>,
    /// The card of a tab (double-click it), while it's open. It gets the keys.
    card: Option<Card>,
    /// The help (Ctrl+Shift+/), while it's open: over everything, it gets the keys.
    help: Option<Help>,
    /// The last click on a tab: a second one on it soon after opens its card.
    last_tab_click: Option<(TabId, Instant)>,
    /// A project held by the mouse in the sidebar.
    sidebar_drag: Option<SidebarDrag>,
    /// The window's title as last set (see `update_window_title`).
    window_title: String,
    /// When the mouse came onto the sidebar button, the sidebar closed: at `PEEK_DELAY` after
    /// it, the sidebar opens (see `peek`).
    burger_since: Option<Instant>,
    /// The bell's flash, until then (see `ring`).
    flash_until: Option<Instant>,
    /// The open sidebar came from resting on the sidebar button, a peek: it closes when the
    /// mouse leaves it. A click or a key in it keeps it, as if opened by hand.
    sidebar_peek: bool,
    /// The window's icon as set last: its colors and size (see `update_icon`).
    icon: Option<IconKey>,
}

/// What the window's icon is made from: the text's, the cursor's and the
/// background's colors, and its size. Another one: the icon is made again.
#[derive(Clone, Copy, PartialEq)]
struct IconKey {
    letters: u32,
    cursor: u32,
    back: u32,
    size: usize,
}

impl App {
    /// Starts the tabs: the project's, or one at home (in a folder: `kel <folder>`).
    /// The window comes later, in `resumed`.
    /// `proxy` is how the tabs' threads wake up the event loop.
    pub fn new(proxy: EventLoopProxy<UserEvent>, launch: project::Launch) -> anyhow::Result<Self> {
        let (project_name, home_folder) = match launch {
            project::Launch::Home => (None, None),
            project::Launch::Project(name) => (Some(name), None),
            project::Launch::Folder(folder) => (None, Some(folder)),
        };
        // A broken config or a missing shell shouldn't stop Kel: start with the
        // defaults and show what went wrong at the top of the screen.
        let mut problems = Vec::new();
        let config = config::load().unwrap_or_else(|e| {
            problems.push(format!("{e:#}"));
            Config::default()
        });
        // A font that isn't installed: the built-in one, and a line that says so.
        let fonts = font::load_family(&config.font).unwrap_or_else(|e| {
            problems.push(format!("{e:#}"));
            font::built_in()
        });
        // A screen with the problems so far in red. The shell starts below them:
        // when ConPTY asks where the cursor is, the answer is the line after them.
        let screen = |problems: &[String]| {
            let mut terminal = Terminal::new(ROWS, COLS);
            terminal.grid.set_scrollback_limit(config.scrollback);
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
        // Without a project: the folder Kel was opened on, else where Kel runs.
        let folder_of = |recipe: &TabRecipe| match &project {
            Some(project) => Some(project.folder_of(recipe)),
            None => home_folder.clone(),
        };

        // Each tab's shell: its own, the project's, or the config's.
        let shell_of = |recipe: &TabRecipe| match &project {
            Some(project) => project.shell_of(recipe, || config.shell()),
            None => config.shell(),
        };
        // The first tab shows the problems. A shell that doesn't start is one more,
        // and the tab gets the system's shell instead.
        let shell = shell_of(&recipes[0]);
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
                Tab::spawn(
                    0,
                    &crate::shell::system(),
                    folder.as_deref(),
                    screen(&problems),
                    proxy.clone(),
                )?
            }
        };
        let mut tabs = vec![first];
        let mut status = None;
        for (id, recipe) in (1..).zip(&recipes[1..]) {
            let (shell, folder) = (shell_of(recipe), folder_of(recipe));
            let spawn = |shell: &str| {
                let mut screen = Terminal::new(ROWS, COLS);
                screen.grid.set_scrollback_limit(config.scrollback);
                Tab::spawn(id, shell, folder.as_deref(), screen, proxy.clone())
            };
            // The others say it in the tab bar, and get the system's shell too.
            let tab = spawn(&shell).or_else(|e| {
                status = Some(format!("can't start shell \"{shell}\": {e:#}"));
                spawn(&crate::shell::system())
            });
            match tab {
                Ok(tab) => tabs.push(tab),
                Err(e) => status = Some(format!("can't open a tab: {e:#}")),
            }
        }
        // Each tab's name, shell and command, by its id: a tab that failed has no place.
        for tab in &mut tabs {
            let recipe = &recipes[tab.id as usize];
            tab.name = recipe.name.clone();
            tab.own_shell = recipe.shell.clone();
            if let Some(command) = &recipe.command {
                tab.type_at_start(command);
            }
        }
        let next_id = recipes.len() as TabId;
        // No window yet: the real layout comes with the first `on_resize`.
        let layout = config.layout.layout(0, 0, 1.0);
        // Home: the start screen. Not over the problems, they're to be read;
        // nor when Kel was opened on a folder, to work there.
        let start = (project_name.is_none() && home_folder.is_none() && problems.is_empty())
            .then(|| Start::new(project::recent_entries(start::RECENT), config.backdrop));

        Ok(App {
            window: None,
            renderer: None,
            tabs,
            active: 0,
            next_id,
            proxy,
            modifiers: ModifiersState::empty(),

            mouse_pos: (0, 0),
            mouse_inside: false,
            mouse_cell: (0, 0),
            selection: None,
            selecting: None,
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
            project: project_name
                .zip(project)
                .map(|(name, project)| OpenProject {
                    name,
                    folder: project.folder,
                    color: project.color,
                    shell: project.shell,
                }),
            parked: Vec::new(),
            palette: None,
            search: None,
            link: None,
            tab_drag: None,
            sidebar: None,
            start,
            card: None,
            help: None,
            last_tab_click: None,
            sidebar_drag: None,
            window_title: String::new(),
            burger_since: None,
            flash_until: None,
            sidebar_peek: false,
            icon: None,
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

    /// The tab `id`, on screen or in a project out of sight. The tabs' threads
    /// only know the id: the output, the answers (`UserEvent::Reply`) and the
    /// end of a shell come for any tab, and an answer that's lost can leave a
    /// program waiting for it.
    fn tab_anywhere(&mut self, id: TabId) -> Option<&mut Tab> {
        self.all_tabs().find(|tab| tab.id == id)
    }

    /// Every tab: on screen, and in the projects open behind.
    fn all_tabs(&mut self) -> impl Iterator<Item = &mut Tab> {
        let parked = self.parked.iter_mut().flat_map(|w| w.tabs.iter_mut());
        self.tabs.iter_mut().chain(parked)
    }

    /// An empty screen for a new tab, keeping the config's `scrollback`.
    fn screen(&self, rows: usize, cols: usize) -> Terminal {
        let mut terminal = Terminal::new(rows, cols);
        terminal.grid.set_scrollback_limit(self.config.scrollback);
        terminal
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
        let screen = self.screen(rows, cols);
        let cwd = self.tab().cwd();
        match Tab::spawn(
            id,
            &self.shell(),
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

    /// A tab's program rang the bell (`on_screen`: the tab on screen). Kel in
    /// front: you see it already, or the tab's dot will say; with `flash`, the
    /// terminal flashes. Kel behind other windows: its taskbar button blinks
    /// until you come back.
    fn ring(&mut self, on_screen: bool) {
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
    fn shell(&self) -> String {
        self.project
            .as_ref()
            .and_then(|project| project.shell.clone())
            .unwrap_or_else(|| self.config.shell())
    }

    /// The shell of tab `id` ended (`exit`, or Ctrl+Shift+W): the tab goes away.
    /// The last one on screen: see `leave_empty`.
    fn remove_tab(&mut self, id: TabId, event_loop: &ActiveEventLoop) {
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

    /// A tab out of sight ended (`exit` in it, or its command closed the shell).
    /// Its project's last tab: the project closes, its file stays as it was.
    fn remove_parked_tab(&mut self, id: TabId) {
        for (w, workspace) in self.parked.iter_mut().enumerate() {
            let Some(index) = workspace.tabs.iter().position(|tab| tab.id == id) else {
                continue;
            };
            workspace.tabs.remove(index);
            if workspace.tabs.is_empty() {
                self.parked.remove(w);
            } else if index < workspace.active || workspace.active == workspace.tabs.len() {
                workspace.active -= 1;
            }
            self.request_redraw(); // the sidebar may show it
            return;
        }
    }

    /// The last tab on screen ended. A project's: back home, the home that was
    /// left or a new one; its file isn't written, it stays as it was, not a
    /// project of no tabs (see `save_project`).
    /// Home's: the project you were in last comes on screen. A tab closed is a
    /// small thing: it doesn't end the other projects' shells. Home comes back,
    /// a new one, when you go there. Nothing else open: Kel closes, as a
    /// terminal does with its last tab.
    /// Returns `false` when Kel should close.
    fn leave_empty(&mut self) -> bool {
        if self.project.is_some() {
            let Ok(home) = self.home_workspace() else {
                return false;
            };
            self.enter(home);
            return true;
        }
        // In home, only projects are out of sight; the last one left is last.
        let Some(workspace) = self.parked.pop() else {
            return false;
        };
        self.enter(workspace);
        true
    }

    /// Home's tabs: the ones left out of sight, or a new plain one.
    fn home_workspace(&mut self) -> anyhow::Result<Workspace> {
        match self.parked.iter().position(|w| w.project.is_none()) {
            Some(index) => Ok(self.parked.remove(index)),
            None => self.start_workspace(None),
        }
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
        // A tab came or went: the tabs and the + moved, maybe under the mouse.
        self.update_hovered();
        // The selection, the search and the half-scrolled line belonged to the
        // other tab.
        self.selection = None;
        self.search = None;
        self.link = None;
        self.scroll_rest = 0.0;
        // Another tab (a new one, a click on one): the shell, not the start screen,
        // and not a card of the tab before.
        self.start = None;
        self.card = None;
        self.update_focus();
        self.request_redraw();
    }

    /// Moves the tab at `from` to the place `to`; the ones between shift over by
    /// one. The tab on screen stays on screen, wherever it goes. Nothing at the
    /// ends: the first tab doesn't go round to the last place.
    fn move_tab(&mut self, from: usize, to: usize) {
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
    fn drag_tab(&mut self) {
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
    fn update_focus(&mut self) {
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
    fn close_tab(&mut self) {
        self.close_tab_at(self.active, "Ctrl+Shift+W");
    }

    /// Ends the shell of the tab at `index`. If a command runs (nvim, `cargo run`),
    /// only on the second press of `how` (a key, or the tab's ×).
    fn close_tab_at(&mut self, index: usize, how: &str) {
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
    fn close_window(&mut self) {
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

    /// Writes the file of the project on screen: its tabs as they are now (see
    /// `save_tabs`). When you leave it and when the window closes: after `exit`
    /// in each tab, the file stays as it was, not a project of no tabs.
    fn save_project(&self) -> anyhow::Result<()> {
        match &self.project {
            Some(open) => save_tabs(open, &self.tabs),
            None => Ok(()), // no project, or one that couldn't be read: not written over
        }
    }

    /// Writes the files of the projects out of sight: all of them, even after one
    /// fails. The first problem is the one returned.
    fn save_parked(&self) -> anyhow::Result<()> {
        let mut saved = Ok(());
        for workspace in &self.parked {
            if let Some(open) = &workspace.project {
                let result = save_tabs(open, &workspace.tabs);
                if saved.is_ok() {
                    saved = result;
                }
            }
        }
        saved
    }

    /// Closes the open project `name`, on screen or out of sight: saves its file
    /// and ends its shells. The one on screen: home comes. If a command runs in
    /// it, or the file can't be written, only the second time (`again` says how).
    fn close_project(&mut self, name: &str, again: &str) {
        let here = self.project.as_ref().is_some_and(|p| p.name == name);
        let (open, tabs) = if here {
            (self.project.as_ref(), &self.tabs)
        } else {
            match self.parked.iter().find(|w| w.name() == Some(name)) {
                Some(w) => (w.project.as_ref(), &w.tabs),
                None => return, // not open
            }
        };
        let (Some(open), Some(first)) = (open, tabs.first()) else {
            return;
        };
        let id = first.id;
        let running = tabs.iter().filter(|tab| tab.command.running()).count();
        let saved = save_tabs(open, tabs);
        let message = match (&saved, running) {
            (Err(e), _) => format!("{e:#}: {again}"),
            (Ok(()), 0) => String::new(),
            (Ok(()), 1) => format!("a command runs there: {again}"),
            (Ok(()), n) => format!("commands run in {n} tabs: {again}"),
        };
        let ask = saved.is_err() || running > 0;
        if self.confirm_close(Closing::Project(id), ask, message) {
            self.end_project(name);
        }
    }

    /// Ends the shells of the open project `name`, without writing its file. Out
    /// of sight: dropped. On screen: home comes, and if home can't start, the
    /// project stays (`false`).
    fn end_project(&mut self, name: &str) -> bool {
        let here = self.project.as_ref().is_some_and(|p| p.name == name);
        if !here {
            // Dropped, its tabs end their shells (`Tab`'s `drop`).
            self.parked.retain(|w| w.name() != Some(name));
            self.request_redraw();
            return true;
        }
        match self.home_workspace() {
            Ok(home) => {
                self.tabs.clear();
                self.enter(home);
                true
            }
            Err(e) => {
                self.set_status(Some(format!("{e:#}")));
                false
            }
        }
    }

    /// Deletes the project `name`: its file, not its folder (`project::delete`).
    /// Always on the second press only (`again` says how): a file deleted can't
    /// come back. An open one is closed first, its commands ended, its file not
    /// written.
    fn delete_project(&mut self, name: &str, again: &str) {
        let here = self.project.as_ref().is_some_and(|p| p.name == name);
        let tabs = if here {
            Some(&self.tabs)
        } else {
            self.parked
                .iter()
                .find(|w| w.name() == Some(name))
                .map(|w| &w.tabs)
        };
        let open = tabs.is_some();
        let running = tabs.map_or(0, |tabs| {
            tabs.iter().filter(|tab| tab.command.running()).count()
        });
        let what = match running {
            // Short: the sidebar's bottom has room for ~30 letters a line.
            0 => "its file goes, folder stays",
            1 => "a command ends, folder stays",
            _ => "commands end, folder stays",
        };
        let message = format!("{again} to delete {name}: {what}");
        if !self.confirm_close(Closing::Delete(name.to_string()), true, message) {
            return;
        }
        if open && !self.end_project(name) {
            return;
        }
        if let Err(e) = project::delete(name) {
            self.set_status(Some(format!("{e:#}")));
        }
        // Home's start screen was made before the file went: again, without it.
        if self.start.is_some() {
            self.start = self.start_screen();
        }
        self.refresh_sidebar();
        self.request_redraw();
    }

    /// What's going on in the project `name` (`None`: home), for the sidebar.
    fn live_of(&self, name: Option<&str>) -> Live {
        let here = self.project.as_ref().map(|p| p.name.as_str()) == name;
        let tabs = if here {
            &self.tabs
        } else {
            match self.parked.iter().find(|w| w.name() == name) {
                Some(w) => &w.tabs,
                None => return Live::default(), // not open
            }
        };
        Live {
            open: true,
            busy: tabs.iter().any(|tab| tab.command.running()),
            activity: tabs.iter().any(|tab| tab.activity),
        }
    }

    // -----------------------------------------------------------------------
    // Projects in one window
    // -----------------------------------------------------------------------

    /// Ctrl+Alt+→ / ←: the next / previous open project (`step` 1 / -1). In the
    /// sidebar's order, which you set, not the order they were opened in: one
    /// that goes out of sight keeps its place. Home first, always there (a new
    /// one if it was closed); round: after the last one, home.
    fn step_project(&mut self, step: isize) {
        let here = self.project.as_ref().map(|p| p.name.clone());
        let open = |name: &String| {
            here.as_ref() == Some(name) || self.parked.iter().any(|w| w.name() == Some(name))
        };
        let mut ring = vec![None];
        ring.extend(project::names().into_iter().filter(open).map(Some));
        let at = ring.iter().position(|name| *name == here).unwrap_or(0);
        let next = (at as isize + step).rem_euclid(ring.len() as isize) as usize;
        if next != at {
            let name = ring[next].clone();
            self.switch_to(name.as_deref());
        }
    }

    /// Goes to the project `name` in this window, or home (`None`). The one on
    /// screen is saved and goes out of sight, everything in it still running.
    /// The other one comes back as it was left, or starts from its file if it
    /// isn't open yet.
    fn switch_to(&mut self, name: Option<&str>) {
        if self.project.as_ref().map(|p| p.name.as_str()) == name {
            // It's on screen. Home again: back to its start screen, position 0.
            if self.start.is_none() {
                self.start = self.start_screen();
                self.request_redraw();
            }
            return;
        }
        let open = self.parked.iter().position(|w| w.name() == name);
        let next = match open {
            Some(index) => self.parked.remove(index),
            None => match self.start_workspace(name) {
                Ok(workspace) => workspace,
                Err(e) => {
                    self.set_status(Some(format!("{e:#}")));
                    return;
                }
            },
        };
        // Can't be saved: say so, and go anyway. The file is tried again when
        // you leave next time, and when the window closes.
        let saved = self.save_project();
        let left = Workspace {
            project: self.project.take(),
            tabs: std::mem::take(&mut self.tabs),
            active: self.active,
        };
        self.start = None;
        self.parked.push(left);
        self.enter(next);
        if let Err(e) = saved {
            self.set_status(Some(format!("{e:#}")));
        }
    }

    /// `workspace` comes on screen: its tabs, its project's name and color.
    fn enter(&mut self, workspace: Workspace) {
        self.project = workspace.project;
        self.tabs = workspace.tabs;
        self.active = workspace.active.min(self.tabs.len() - 1);
        // What was waiting for a second press or held by the mouse was the
        // other project's.
        self.pending_close = None;
        self.tab_drag = None;
        self.set_status(None);
        self.on_resize(); // room for this project's name before the tabs
        self.show_tab(self.active);
        // After `show_tab`, which puts a start screen away.
        self.start = self.start_screen();
    }

    /// Home's start screen as you come back to it, whatever you did there
    /// before: the projects used last, as they are now. Home is position 0, a
    /// calm place to start from. `None` in a project, and while a command runs
    /// in the tab on screen: you came back to watch it. Under the screen, the
    /// terminal stays as it was (a key puts the screen away).
    fn start_screen(&self) -> Option<Start> {
        let idle = !self.tab().command.running();
        (self.project.is_none() && idle)
            .then(|| Start::new(project::recent_entries(start::RECENT), self.config.backdrop))
    }

    /// The tabs of the project `name` (`None`: home, one plain tab), started now:
    /// as `kel <name>` would start them, as big as the tabs on screen.
    fn start_workspace(&mut self, name: Option<&str>) -> anyhow::Result<Workspace> {
        let project = name.map(project::load).transpose()?;
        let recipes = project
            .as_ref()
            .map_or_else(|| vec![TabRecipe::default()], Project::tabs);
        let (rows, cols) = match self.tabs.first() {
            Some(tab) => {
                let terminal = tab.terminal.lock().unwrap();
                (terminal.grid.rows(), terminal.grid.cols())
            }
            None => (ROWS, COLS),
        };
        let mut tabs = Vec::new();
        let mut failed = None;
        for recipe in &recipes {
            let folder = project.as_ref().map(|p| p.folder_of(recipe));
            let shell = match &project {
                Some(project) => project.shell_of(recipe, || self.config.shell()),
                None => self.config.shell(),
            };
            let id = self.next_id;
            self.next_id += 1;
            let screen = self.screen(rows, cols);
            match Tab::spawn(id, &shell, folder.as_deref(), screen, self.proxy.clone()) {
                Ok(mut tab) => {
                    tab.name = recipe.name.clone();
                    tab.own_shell = recipe.shell.clone();
                    if let Some(command) = &recipe.command {
                        tab.type_at_start(command);
                    }
                    tabs.push(tab);
                }
                Err(e) => failed = Some(e),
            }
        }
        if tabs.is_empty() {
            let e = failed.unwrap_or_else(|| anyhow::anyhow!("no tabs"));
            return Err(e.context("can't open a tab"));
        }
        if let Some(e) = failed {
            self.set_status(Some(format!("can't open a tab: {e:#}")));
        }
        let project = name.zip(project).map(|(name, project)| OpenProject {
            name: name.to_string(),
            folder: project.folder,
            color: project.color,
            shell: project.shell,
        });
        // Home's start screen comes with `enter`.
        Ok(Workspace {
            project,
            tabs,
            active: 0,
        })
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

    /// The tab on screen wants a solid background (see `Chrome::solid`): a
    /// full-screen program runs in it, or Claude Code, which draws in the normal
    /// screen (known by its icon, while a command runs: not a tab only called
    /// "claude").
    fn solid_under(&self) -> bool {
        if !self.config.glass.solid_under_apps {
            return false;
        }
        let tab = self.tab();
        let full_screen = tab.terminal.lock().unwrap().grid.alternate();
        full_screen || tab.command.running() && tab.icon() == "claude"
    }

    /// The window's own title, seen in the taskbar and in Alt+Tab: the tab on screen.
    /// Also on each draw, as a tab's label changes when its command gets busy:
    /// set only when it's different.
    fn update_window_title(&mut self) {
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
                // Every tab keeps the new number of lines, the ones behind too.
                if new.scrollback != self.config.scrollback {
                    for tab in self.all_tabs() {
                        let mut terminal = tab.terminal.lock().unwrap();
                        terminal.grid.set_scrollback_limit(new.scrollback);
                    }
                    self.request_redraw();
                }
                // Another picture: shown at once, if the start screen is up.
                let backdrop = new.backdrop != self.config.backdrop;
                self.config = new;
                if backdrop && self.start.is_some() {
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
            Action::MoveTabLeft => self.move_tab(self.active, self.active.saturating_sub(1)),
            Action::MoveTabRight => self.move_tab(self.active, self.active + 1),
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
        }
    }

    /// Saves the tabs as the new project `name`, in the folder of the tab on
    /// screen (the other tabs' folders are kept inside it, see `save_project`).
    /// The window becomes that project: its name shows, it's saved when it
    /// closes. Its color, if this window had one, stays.
    fn save_as_project(&mut self, name: String) {
        if project::exists(&name) {
            // Made since the palette opened: never written over.
            self.set_status(Some(format!("project \"{name}\" already exists")));
            return;
        }
        let Some(folder) = self.tab().cwd() else {
            self.set_status(Some("can't save: no folder for this tab".to_string()));
            return;
        };
        // Its color and shell, if this window had them, stay.
        let color = self.project.as_ref().and_then(|p| p.color);
        let shell = self.project.as_ref().and_then(|p| p.shell.clone());
        let before = self.project.replace(OpenProject {
            name,
            folder,
            color,
            shell,
        });
        if let Err(e) = self.save_project() {
            self.project = before;
            self.set_status(Some(format!("{e:#}")));
            return;
        }
        self.update_window_title();
        self.on_resize(); // room for the name before the tabs
        self.request_redraw();
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
        let tabs = self.tabs.len();
        self.new_tab();
        if self.tabs.len() > tabs {
            let tab = self.tabs.last_mut().unwrap();
            tab.type_when_ready(&setup::command(&path));
        }
    }

    /// A key while the sidebar is open: Up/Down choose a row, Enter does it, Esc
    /// closes. Everything else waits until it's closed.
    fn sidebar_key(&mut self, event: &KeyEvent) {
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
    fn open_tab_card(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let name = tab.name.as_deref();
        let command = tab.start_command.as_deref();
        self.card = Some(Card::tab(tab.id, name, command, self.project.is_some()));
        self.palette = None;
        self.search = None;
        self.sidebar = None;
        self.request_redraw();
    }

    /// Where the open card is: under its tab.
    fn card_rect(&self) -> Option<Rect> {
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
    fn card_key(&mut self, event: &KeyEvent) {
        let ctrl = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();
        let Some(card) = &mut self.card else {
            return;
        };
        card.error = None;
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.card = None,
            Key::Named(NamedKey::ArrowLeft) if card.step_swatch(-1) => {}
            Key::Named(NamedKey::ArrowRight) if card.step_swatch(1) => {}
            Key::Named(NamedKey::Enter) => self.apply_card(),
            Key::Named(NamedKey::Tab) => card.next_field(if shift { -1 } else { 1 }),
            Key::Named(NamedKey::ArrowUp) => card.next_field(-1),
            Key::Named(NamedKey::ArrowDown) => card.next_field(1),
            Key::Named(NamedKey::ArrowLeft) => card.focused().move_cursor(-1),
            Key::Named(NamedKey::ArrowRight) => card.focused().move_cursor(1),
            Key::Named(NamedKey::Home) => card.focused().jump(false),
            Key::Named(NamedKey::End) => card.focused().jump(true),
            Key::Named(NamedKey::Backspace) if ctrl => card.focused().backspace_word(),
            Key::Named(NamedKey::Backspace) => card.focused().backspace(),
            Key::Named(NamedKey::Delete) => card.focused().delete(),
            // Ctrl+V (or Kel's Ctrl+Shift+V): the clipboard, as one line.
            _ if ctrl && event.physical_key == PhysicalKey::Code(KeyCode::KeyV) => {
                if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                    card.type_text(&text);
                }
            }
            // Typed letters, not Ctrl+letter (see `palette_key`).
            _ if !ctrl => {
                if let Some(text) = event.text.as_deref() {
                    card.type_text(text);
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    /// Enter, or a click outside the card: what's typed is kept. A tab's name and
    /// command go into the project's file at once. The command runs the next
    /// time the project opens, not now.
    fn apply_card(&mut self) {
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
        }
        if let Err(e) = self.save_project() {
            self.set_status(Some(format!("{e:#}")));
        }
        self.update_window_title();
        self.request_redraw();
    }

    /// Opens the card of the project `name`, under its row in the sidebar: its
    /// name and color. The sidebar opens first if it isn't open.
    fn open_project_card(&mut self, name: &str) {
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

    /// The project `old` is `new` now, and `color`: its file is renamed and
    /// written. One open in the window takes them at once: its name before the
    /// tabs, its color.
    fn change_project(&mut self, old: &str, new: &str, color: Option<u32>) -> anyhow::Result<()> {
        if new != old {
            project::rename(old, new)?;
            let parked = self.parked.iter_mut().filter_map(|w| w.project.as_mut());
            for open in self.project.iter_mut().chain(parked) {
                if open.name == old {
                    open.name = new.to_string();
                }
            }
        }
        if let Some(open) = self.project.as_mut().filter(|p| p.name == new) {
            open.color = color;
            self.save_project()?;
            self.update_window_title();
            self.on_resize(); // the name before the tabs: another length
            return Ok(());
        }
        for workspace in &mut self.parked {
            if let Workspace {
                project: Some(open),
                tabs,
                ..
            } = workspace
                && open.name == new
            {
                open.color = color;
                return save_tabs(open, tabs);
            }
        }
        project::set_color(new, color)
    }

    /// The sidebar again, from the files: a project renamed, closed, or another
    /// one on screen. The same row stays chosen.
    fn refresh_sidebar(&mut self) {
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
    fn sidebar_edit(&mut self) {
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
    fn start_key(&mut self, event: &KeyEvent, shortcut: Option<&Action>) -> bool {
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
    fn save_sidebar_order(&mut self) {
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
    fn sidebar_choose(&mut self) {
        if let Some(sidebar) = self.sidebar.take() {
            self.run(sidebar.chosen());
            self.request_redraw();
        }
    }

    /// A key while the search is open: it's for the search, never the shell.
    /// Letters go into its box (and it finds again right away), Enter or Up goes
    /// to the match before (older output), Shift+Enter or Down to the next one,
    /// Esc closes. Ctrl+Shift+V pastes into the box.
    fn search_key(&mut self, event: &KeyEvent, shortcut: Option<Action>) {
        let Some(search) = &mut self.search else {
            return;
        };
        let back = if self.modifiers.shift_key() { 1 } else { -1 };
        match &event.logical_key {
            _ if shortcut == Some(Action::Paste) => {
                if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
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
            _ if !self.modifiers.control_key() => {
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
    fn find(&mut self, keep_place: bool) {
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
    fn palette_key(&mut self, event: &KeyEvent) {
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
            // Ctrl+D would type a "d".
            _ if !self.modifiers.control_key() => {
                if let Some(text) = event.text.as_deref() {
                    let text: String = text.chars().filter(|c| !c.is_control()).collect();
                    palette.type_text(&text);
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    /// Pastes the clipboard into the shell. A program that takes pasted text as
    /// a whole (bracketed paste: nvim, a newer PSReadLine) gets it as it is.
    /// Else each line would run as it comes: one line goes in without its line
    /// break, and several ask first, in the tab bar ("paste 2 lines?"), as
    /// Windows Terminal does.
    fn paste(&mut self) {
        let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) else {
            return;
        };
        let bracketed = self.tab().terminal.lock().unwrap().bracketed_paste;
        if bracketed {
            self.send_to_shell(&keys::paste_bytes(&text, true));
            return;
        }
        let (text, lines) = keys::plain_paste(&text);
        let message = format!("Paste {lines} lines? They run. Ctrl+Shift+V again");
        let what = Closing::Paste(text.to_string());
        if self.confirm_close(what, lines > 1, message) {
            self.send_to_shell(&keys::paste_bytes(text, false));
        }
    }

    /// Copies the mouse selection. Without one, it's a Ctrl+C for the shell,
    /// and PowerShell then copies its own keyboard selection.
    fn copy(&mut self) {
        if !self.copy_selection() {
            self.send_to_shell(&[0x03]);
            return;
        }
        self.selection = None;
        self.request_redraw();
    }

    /// Puts the mouse selection's text in the clipboard, if there is one. It
    /// stays selected: `copy` takes it away, a selection copied as the mouse
    /// lets go (`copy_on_select`) stays to be seen.
    fn copy_selection(&mut self) -> bool {
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

    /// Shows the cursor and starts the blink over, so it doesn't vanish while you type.
    fn restart_blink(&mut self) {
        self.cursor_on = true;
        self.next_blink = Instant::now() + BLINK_INTERVAL;
        self.request_redraw();
    }
}

// winit calls these methods when something happens; we never call them ourselves.
/// Writes the project `open`'s file with `tabs`, each in its last folder. Only
/// what you wrote in the file comes back, the name, the command and the shell:
/// Kel never adds a command, so opening a project never runs `git push` by
/// itself. A tab opened since (Ctrl+Shift+T) is saved as a plain one, with its
/// folder.
fn save_tabs(open: &OpenProject, tabs: &[Tab]) -> anyhow::Result<()> {
    // Everything the file had but the tabs, or a line would be lost: the tabs
    // are written again, the rest is kept as it was read.
    let mut project = Project {
        folder: open.folder.clone(),
        color: open.color,
        shell: open.shell.clone(),
        tabs: Vec::new(),
    };
    let tabs = tabs.iter().map(|tab| TabRecipe {
        name: tab.name.clone(),
        folder: tab.cwd().and_then(|cwd| project.tab_folder(&cwd)),
        command: tab.start_command.clone(),
        shell: tab.own_shell.clone(),
    });
    project.tabs = tabs.collect();
    project::save(&open.name, &project)
}

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
                if tab.take_bell() {
                    self.ring(on_screen);
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
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.on_key(event)
            }
            WindowEvent::MouseWheel { delta, .. } => self.on_mouse_wheel(delta),
            WindowEvent::CursorMoved { position, .. } => self.on_mouse_move(position),
            WindowEvent::CursorLeft { .. } => {
                self.mouse_inside = false;
                self.set_hovered(None);
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
            }
        }
        self.peek();
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
    fn request_redraw(&self) {
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
            maximized: self.window.as_ref().is_some_and(|w| w.is_maximized()),
            status: self.status.as_deref(),
            question: self.pending_close.as_ref().map(|p| p.message.as_str()),
            palette: self.palette.as_ref(),
            search: self.search.as_ref(),
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
            renderer.draw(
                &self.tabs[self.active].terminal.lock().unwrap().grid,
                &self.layout,
                &chrome,
                self.selection,
                self.cursor_on,
            );
        }
    }

    /// Whether the keys go to the terminal: the window has them, and nothing
    /// over the terminal takes them (see `on_key`). Else its cursor is hollow.
    fn keys_here(&self) -> bool {
        self.focused
            && self.sidebar.is_none()
            && self.palette.is_none()
            && self.search.is_none()
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
    fn on_resize(&mut self) {
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
        let shortcut = keys::shortcut(event.physical_key, self.modifiers);
        if self.help.is_some() {
            self.help_key(&event, shortcut);
            return;
        }
        // Ctrl+Shift+P and Ctrl+Shift+F still work in the palette and the search:
        // they close one, or swap one for the other.
        let switch = matches!(
            shortcut,
            Some(Action::Palette | Action::Find | Action::Sidebar)
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
        if self.start.is_some() && self.start_key(&event, shortcut.as_ref()) {
            return;
        }
        if let Some(action) = shortcut {
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

    /// A key while the help is open: ↑↓ PgUp PgDn Home End scroll it; Esc or
    /// Ctrl+Shift+/ closes it. The palette, the search and the sidebar take its
    /// place; nothing else gets through to the shell.
    fn help_key(&mut self, event: &KeyEvent, shortcut: Option<Action>) {
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

    /// Mouse wheel or touchpad: scroll the view into the scrollback.
    fn on_mouse_wheel(&mut self, delta: MouseScrollDelta) {
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

        if let Some(from) = self.selecting {
            // Off the pressed cell: the selection starts. Back on it later: it
            // stays, one cell, chosen on purpose.
            match &mut self.selection {
                Some(selection) => selection.end = (row, col),
                None if (row, col) != from => {
                    self.selection = Some(Selection {
                        start: from,
                        end: (row, col),
                    });
                }
                None => return,
            }
            self.request_redraw();
        } else if moved
            && (self.mouse_button.is_some() || self.over_text() && self.program_takes_mouse())
        {
            // Dragging in the program, or just moving if it asked for every move.
            self.report_mouse(mouse::Event::Move(self.mouse_button));
        }
    }

    /// Finds the link under the mouse again, while Ctrl is held: it's underlined
    /// and the mouse becomes a hand. After the mouse moved, or Ctrl changed.
    fn update_link(&mut self) {
        let look = self.modifiers.control_key()
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
    /// as clicking it in another program would; a file shown in Explorer, its
    /// folder open and the file chosen (a folder: opened). Nothing is run.
    /// `rundll32` and not `explorer` for a page: explorer cuts its argument at
    /// commas, and links have them.
    fn open_link(&mut self, url: &str) {
        use std::process::Command;
        let opened = match links::target(url) {
            Some(links::Target::Web) => Command::new("rundll32")
                .args(["url.dll,FileProtocolHandler", url])
                .spawn(),
            Some(links::Target::File(path)) if path.is_dir() => {
                Command::new("explorer").arg(&path).spawn()
            }
            Some(links::Target::File(path)) if path.exists() => {
                // `/select,"C:\a, b.txt"`: in quotes, so a comma in the name stays.
                #[cfg(windows)]
                use std::os::windows::process::CommandExt;
                let select = format!("/select,\"{}\"", path.display());
                Command::new("explorer").raw_arg(select).spawn()
            }
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
            if let Some(TitleHit::Tab { index, .. }) = hit {
                self.show_tab(index);
                self.open_tab_card(index);
                return;
            }
        }
        // A middle click on a tab closes it, as in a browser.
        if button == MouseButton::Middle && state == ElementState::Pressed {
            let (x, y) = self.mouse_pos;
            let hit = self.layout.title_hit(x, y, self.tabs.len());
            if let Some(TitleHit::Tab { index, .. }) = hit {
                self.close_tab_at(index, "middle click");
                return;
            }
        }
        // Ctrl+click on a link opens it, even in a program that has the mouse.
        // Elsewhere Ctrl+click is what it was.
        if button == MouseButton::Left
            && state == ElementState::Pressed
            && self.modifiers.control_key()
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
    fn update_hovered(&mut self) {
        let (x, y) = self.mouse_pos;
        let hit = if !self.mouse_inside || self.resize_edge.is_some() {
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
    fn peek(&mut self) {
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
    fn leave_peek(&mut self) {
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
            None => self.on_empty_bar_click(&window),
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
            ElementState::Pressed
                if self
                    .layout
                    .title_bar
                    .contains(self.mouse_pos.0, self.mouse_pos.1) =>
            {
                self.on_title_bar_click()
            }
            // The old selection goes; a new one starts when the mouse moves on.
            ElementState::Pressed => {
                if self.selection.take().is_some() {
                    self.request_redraw();
                }
                self.selecting = Some(self.mouse_cell);
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
