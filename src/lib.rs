//! Kel: a small terminal emulator.
//!
//! How the pieces fit together:
//!
//! ```text
//!  keyboard/mouse ──> app ──bytes──> pty (ConPTY) ──> powershell.exe
//!                                                         │
//!  window <── renderer <── term::Grid <── term::Terminal <┘ bytes (text + escape codes)
//!            (pixels)      (cells)        (escape codes → Grid calls)
//! ```
//!
//! - `app`: the window and all events. Owns everything else.
//! - `keys`: Kel's shortcuts, and the bytes other keys send to the shell.
//! - `config`: the settings file, `%APPDATA%\kel\config.toml`.
//! - `fallback`: fonts for the letters the terminal's font lacks (emoji, Chinese).
//! - `font`: finds a font, and turns its letters into pixels.
//! - `icons`: the tab icons, 8 x 8 pixel grids in code, like the logo.
//! - `layout`: where the terminal panel goes in the window.
//! - `mouse`: the bytes that tell a program (nvim) what the mouse does, if it asked.
//! - `open`: opens a page, a file or a folder the way the system does.
//! - `palette`: the command palette's state: what's typed, what's shown and chosen.
//! - `project`: a project file, the recipe for a set of tabs.
//! - `pty`: starts the shell inside a pseudo-terminal.
//! - `shell`: `shell` in the config: the program and its arguments, or the system's.
//! - `term`: understands what the shell sends and keeps the screen as a grid of cells.
//! - `renderer`: turns that grid into pixels.
//! - `search`: the search in the output (Ctrl+Shift+F): what's typed, the matches.
//! - `sidebar`: the panel of projects (the sidebar button, Ctrl+Shift+B).
//! - `card`: the small box for changing a tab's name and command.
//! - `start`: the start screen of a new home: the recent projects.
//! - `help`: all the keys and the settings now (Ctrl+Shift+/).
//! - `links`: `https://...` in the output and programs' own links (OSC 8), opened
//!   with Ctrl+click.
//! - `setup`: the install scripts from `setup/`, built into the exe.
//! - `shell_integration`: makes the shell tell Kel its folder, and when a command
//!   starts and ends.
//! - `tab`: one shell with its screen; `app` keeps a list of them.
//! - `theme`: colors as programs ask for them, and the theme that turns them into pixels.

pub mod app;
pub mod backdrop;
pub mod card;
pub mod config;
pub mod fallback;
pub mod font;
pub mod glass;
pub mod help;
pub mod icons;
pub mod keys;
pub mod layout;
pub mod links;
pub mod logo;
pub mod mouse;
pub mod open;
pub mod palette;
pub mod project;
pub mod pty;
pub mod renderer;
pub mod search;
pub mod setup;
pub mod shell;
pub mod shell_integration;
pub mod sidebar;
pub mod start;
pub mod tab;
pub mod term;
pub mod theme;
