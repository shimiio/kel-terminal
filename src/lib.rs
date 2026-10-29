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
//! - `icons`: the tab icons, pixel-art PNGs built into the .exe.
//! - `layout`: where the terminal panel goes in the window.
//! - `pty`: starts the shell inside a pseudo-terminal.
//! - `term`: understands what the shell sends and keeps the screen as a grid of cells.
//! - `renderer`: turns that grid into pixels.
//! - `tab`: one shell with its screen; `app` keeps a list of them.
//! - `theme`: colors as programs ask for them, and the theme that turns them into pixels.

pub mod app;
pub mod config;
pub mod icons;
pub mod keys;
pub mod layout;
pub mod pty;
pub mod renderer;
pub mod tab;
pub mod term;
pub mod theme;
