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
//! - `config`: the settings file, `%APPDATA%\kel\config.toml`.
//! - `layout`: where the terminal panel goes in the window.
//! - `pty`: starts the shell inside a pseudo-terminal.
//! - `term`: understands what the shell sends and keeps the screen as a grid of cells.
//! - `renderer`: turns that grid into pixels.
//! - `theme`: colors as programs ask for them, and the theme that turns them into pixels.

pub mod app;
pub mod config;
pub mod layout;
pub mod pty;
pub mod renderer;
pub mod term;
pub mod theme;
