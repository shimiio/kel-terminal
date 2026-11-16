//! The application: owns the window and the tabs, and reacts to every event.
//!
//! The **main thread** runs the winit event loop: keys, mouse, resize, redraw.
//! Each tab has its own shell and two threads of its own (see `tab`), which
//! report back with `UserEvent`s.
//!
//! Here: `App` with all it keeps, and how it starts (`new`). What it does is in
//! the files of `app/`, each with a part of `impl App`:
//!
//! - `events`: what winit tells Kel (a key, a resize, a shell's output, time to draw).
//! - `mouse`: moves, buttons, the wheel: selecting, links, the title bar.
//! - `tabs`: a new tab, closing, showing, moving one; closing the window.
//! - `workspaces`: home and the open projects: switching, saving, closing, deleting.
//! - `overlays`: what opens over the terminal and takes the keys (the sidebar,
//!   the cards, the search, the ask box, the palette, the start screen, the help).
//! - `actions`: what each shortcut does, the clipboard, the config read again.

mod actions;
mod events;
mod mouse;
mod overlays;
mod tabs;
mod workspaces;

use crate::ask::{Answer, Ask};
use crate::card::Card;
use crate::config::{self, Config};
use crate::font::{self, Fonts};
use crate::help::Help;
use crate::history;
use crate::layout::{Layout, TitleHit};
use crate::palette::Palette;
use crate::project::{self, Project, TabRecipe};
use crate::renderer::Renderer;
use crate::search::Search;
use crate::session;
use crate::sidebar::Sidebar;
use crate::start::{self, Start};
use crate::tab::{Tab, TabId};
use crate::term::{Link, Terminal};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};
use vte::Parser;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, ResizeDirection, Window};

// Starting size, in cells. After that, the size comes from the window (`on_resize`).
const ROWS: usize = 24;
const COLS: usize = 80;
/// Two clicks closer than this are a double click (the Windows default).
const DOUBLE_CLICK: Duration = Duration::from_millis(500);
/// How long the cursor stays on, then off.
const BLINK_INTERVAL: Duration = Duration::from_millis(500);
/// How long a hidden tab bar (`tab_bar = "hover"`) shows by itself, when the tab
/// on screen changes or a command in another tab ends: long enough to read
/// where you are.
const BAR_TIME: Duration = Duration::from_millis(1500);
/// How long Ctrl is held after Ctrl+Tab before the list of tabs shows (see
/// `App::switcher`): a quick Ctrl+Tab only changes the tab, nothing flickers.
const SWITCHER_DELAY: Duration = Duration::from_millis(150);
/// How long the mouse rests on the sidebar button before the sidebar opens by itself: long
/// enough that passing over it on the way to a tab doesn't.
const PEEK_DELAY: Duration = Duration::from_millis(400);

/// The selected area, from where the mouse was pressed to where it is now. (row, col)
/// The rows are lasting ones (see `Grid::dropped`), not the view's: the
/// selection stays on its text when the view scrolls, or new output pushes it up.
#[derive(Clone, Copy, Debug, PartialEq)]
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

    /// From the first cell of the two to the last: the word clicked twice and
    /// the one the mouse is on now, with all between them.
    fn joined(self, (start, end): ((usize, usize), (usize, usize))) -> Selection {
        let (first, last) = self.ordered();
        Selection {
            start: first.min(start),
            end: last.max(end),
        }
    }
}

/// The list of tabs: which tabs there are, and which one is on screen. Two
/// ways in:
/// - Ctrl held after Ctrl+Tab, where the tabs aren't in sight (`tab_bar` is
///   `"title"` or `"never"`): it goes when Ctrl is let go;
/// - a click on the title, or the palette's line: `pinned`, it stays until a
///   tab is clicked in it, Esc, Enter, or a click or a key elsewhere.
#[derive(Clone, Copy)]
struct Switcher {
    /// When it shows: `SWITCHER_DELAY` after the first Ctrl+Tab.
    at: Instant,
    /// It shows already.
    shown: bool,
    pinned: bool,
}

/// The left button, held in the terminal (see `on_left_button`).
#[derive(Clone, Copy)]
struct Selecting {
    /// The cell it was pressed on, by lasting row (as a `Selection`'s).
    from: (usize, usize),
    by: By,
}

/// What a drag selects: letters after one click, whole words after two (from
/// the word clicked to the one under the mouse), whole lines after three.
#[derive(Clone, Copy)]
enum By {
    Cell,
    Word(Selection),
    Line(Selection),
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
    /// The model answered the question of this number (see `ask`): a command,
    /// or why there's none.
    Answer(u64, Result<Answer, String>),
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
    /// The mouse selection, if any. By lasting rows, so it follows its text.
    selection: Option<Selection>,
    /// While the left button is held: the cell it was pressed on. After one
    /// click, a selection starts only once the mouse leaves that cell: a click
    /// selects nothing. Two select the word, three the line.
    selecting: Option<Selecting>,
    /// The terminal's last click: when, where and which one in a row it was
    /// (1, 2 or 3: a double click, a triple one).
    text_click: Option<(Instant, (usize, usize), u8)>,
    /// The button whose press went to the program (see `on_mouse_button`): its
    /// moves and its release go there too.
    mouse_button: Option<crate::mouse::Button>,
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
    /// The mouse is over the title bar: buttons set to `hover` show.
    mouse_on_bar: bool,
    /// The mouse called a hidden tab bar (`tab_bar = "hover"`): it stays while
    /// the mouse is on it (see `update_hovered`).
    bar_peek: bool,
    /// A hidden tab bar shows by itself until then (see `show_bar`).
    bar_until: Option<Instant>,
    /// The list of tabs (see `Switcher`), while it's open.
    switcher: Option<Switcher>,
    /// The mouse's shape as last set (see `update_cursor`).
    cursor_icon: CursorIcon,
    /// When the empty title bar was last clicked, to notice a double click.
    last_click: Option<Instant>,
    /// The window is closing (see `close_window`): `about_to_wait` ends the event loop.
    quit: bool,
    /// `KEL_FRAMES=1`: how long frames take, on stderr.
    frames: crate::frames::FrameLog,
    /// Cmd+H on a Mac: hide Kel when the loop next can (it needs the loop).
    hide: bool,
    /// A close that asked "sure?" and waits for the second press.
    pending_close: Option<PendingClose>,
    /// Why a key did nothing, and until when it says so: where a question goes,
    /// for `CONFIRM_TIME`. Not a problem, so not the status, which stays.
    notice: Option<(String, Instant)>,
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
    /// The box that asks a model for a command, while it's open. It gets the keys.
    ask: Option<Ask>,
    /// How many questions went to the model: each one's number (see `Ask::sent`).
    asked: u64,
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
        // Above them, the tab's output from last time (`last`), if it has some.
        let screen = |problems: &[String], last: Option<&session::Last>| {
            let mut terminal = Terminal::new(ROWS, COLS);
            terminal.grid.set_scrollback_limit(config.scrollback);
            if let Some(last) = last {
                terminal.show_last_time(&last.text, &last.when);
            }
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
        // Each tab's history of commands: a project's own, home's the shell's.
        let history_of = |recipe: &TabRecipe| {
            let name = project.as_ref().and(project_name.as_deref())?;
            history::file(name, recipe.name.as_deref())
        };
        // Each tab's output from last time, a project's (see `session`): by the
        // tab's name, or its place in the bar from 1.
        let last_of = |place: usize, recipe: &TabRecipe| {
            let name = project.as_ref().and(project_name.as_deref())?;
            let keeps = config.restore_output;
            keeps.then(|| session::load(name, recipe.name.as_deref(), place))?
        };
        // The first tab shows the problems. A shell that doesn't start is one more,
        // and the tab gets the system's shell instead.
        let shell = shell_of(&recipes[0]);
        let folder = folder_of(&recipes[0]);
        let history = history_of(&recipes[0]);
        let last = last_of(1, &recipes[0]);
        let first = match Tab::spawn(
            0,
            &shell,
            folder.as_deref(),
            history.as_deref(),
            config.suggestions,
            screen(&problems, last.as_ref()),
            proxy.clone(),
        ) {
            Ok(tab) => tab,
            Err(e) => {
                problems.push(format!("can't start shell \"{shell}\": {e:#}"));
                Tab::spawn(
                    0,
                    &crate::shell::system(),
                    folder.as_deref(),
                    history.as_deref(),
                    config.suggestions,
                    screen(&problems, last.as_ref()),
                    proxy.clone(),
                )?
            }
        };
        let mut tabs = vec![first];
        let mut status = None;
        for (id, recipe) in (1..).zip(&recipes[1..]) {
            let (shell, folder) = (shell_of(recipe), folder_of(recipe));
            let history = history_of(recipe);
            let last = last_of(id as usize + 1, recipe);
            let spawn = |shell: &str| {
                let mut screen = Terminal::new(ROWS, COLS);
                screen.grid.set_scrollback_limit(config.scrollback);
                if let Some(last) = &last {
                    screen.show_last_time(&last.text, &last.when);
                }
                let (folder, history) = (folder.as_deref(), history.as_deref());
                Tab::spawn(
                    id,
                    shell,
                    folder,
                    history,
                    config.suggestions,
                    screen,
                    proxy.clone(),
                )
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
                tab.type_at_start(command, recipe.start);
            }
        }
        let next_id = recipes.len() as TabId;
        // No window yet: the real layout comes with the first `on_resize`.
        let layout = config.layout.layout(0, 0, 1.0);
        // Home: the start screen. Not over the problems, they're to be read;
        // nor when Kel was opened on a folder, to work there; nor turned off.
        let start = (config.start_screen
            && project_name.is_none()
            && home_folder.is_none()
            && problems.is_empty())
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
            text_click: None,
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
            mouse_on_bar: false,
            bar_peek: false,
            bar_until: None,
            switcher: None,
            cursor_icon: CursorIcon::Default,
            last_click: None,
            quit: false,
            frames: crate::frames::FrameLog::from_env(),
            hide: false,
            pending_close: None,
            notice: None,
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
            ask: None,
            asked: 0,
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
        self.tab_mut().send_typed(bytes);
    }
}

/// `option_as_alt` as the Mac's window takes it: both Options are Alt (the Mac
/// gives the plain letter then), or neither (they type `@`, `{`, `å`).
#[cfg(target_os = "macos")]
fn option_as_alt(on: bool) -> winit::platform::macos::OptionAsAlt {
    use winit::platform::macos::OptionAsAlt;
    if on {
        OptionAsAlt::Both
    } else {
        OptionAsAlt::None
    }
}
