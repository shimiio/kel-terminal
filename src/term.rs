//! The terminal itself, without any window or pixels.
//!
//! - `terminal`: reads the shell's output (text + escape codes) and decides what to do.
//! - `grid`: the screen as rows of cells, plus the cursor, colors and scrollback.
//!
//! Nothing here draws, so it can be tested with plain strings (see the tests in terminal.rs).

mod grid;
mod terminal;

pub use grid::{Cell, Grid};
pub use terminal::Terminal;
