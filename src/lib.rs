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
//! - `icons`: the tab icons, pixel-art PNGs built into the .exe.
//! - `layout`: where the terminal panel goes in the window.
//! - `mouse`: the bytes that tell a program (nvim) what the mouse does, if it asked.
//! - `palette`: the command palette's state: what's typed, what's shown and chosen.
//! - `project`: a project file, the recipe for a set of tabs.
//! - `pty`: starts the shell inside a pseudo-terminal.
//! - `term`: understands what the shell sends and keeps the screen as a grid of cells.
//! - `renderer`: turns that grid into pixels.
//! - `setup`: the install scripts from `setup/`, built into the exe.
//! - `shell_integration`: makes the shell tell Kel its folder, and when a command
//!   starts and ends.
//! - `tab`: one shell with its screen; `app` keeps a list of them.
//! - `theme`: colors as programs ask for them, and the theme that turns them into pixels.

pub mod app;
pub mod config;
pub mod fallback;
pub mod icons;
pub mod keys;
pub mod layout;
pub mod mouse;
pub mod palette;
pub mod project;
pub mod pty;
pub mod renderer;
pub mod setup;
pub mod shell_integration;
pub mod tab;
pub mod term;
pub mod theme;
