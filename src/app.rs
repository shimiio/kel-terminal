//! The application: owns the window, the shell and the terminal state,
//! and reacts to every event.
//!
//! Two threads:
//! - the **main thread** runs the winit event loop: keys, mouse, resize, redraw.
//! - the **reader thread** (`spawn_reader`) waits for shell output and parses it
//!   into the grid, then sends `UserEvent::PtyOutput` so the main thread redraws.
//!
//! Both need the `Terminal`, so it lives in an `Arc<Mutex<..>>`:
//! `Arc` lets both threads own it, `Mutex` lets only one use it at a time.
//! Lock it, use it, let the lock go. Locking it twice at once freezes the app (deadlock).

use crate::pty;
use crate::renderer::{FONT_SIZE, Renderer};
use crate::term::Terminal;
use portable_pty::{Child, MasterPty, PtySize};
use std::io::{Read, Write};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use vte::Parser;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ControlFlow;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
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

/// Custom events sent from other threads to wake up the event loop.
#[derive(Debug)]
pub enum UserEvent {
    /// The reader thread parsed new shell output into the grid.
    PtyOutput,
    /// The program asked the terminal something; these bytes are the answer.
    Reply(Vec<u8>),
    /// The shell closed its output.
    PtyExited,
}

pub struct App {
    // Created in `resumed`, so they are `None` until the window exists.
    window: Option<Rc<Window>>,
    renderer: Option<Renderer>,
    /// The screen state, shared with the reader thread.
    terminal: Arc<Mutex<Terminal>>,
    /// Which of Shift/Ctrl/Alt are held right now (updated by `ModifiersChanged`).
    modifiers: ModifiersState,
    /// Bytes written here reach the shell as if they were typed.
    writer: Box<dyn Write + Send>,
    /// Our end of the PTY; used to tell the shell about a new size.
    master: Box<dyn MasterPty + Send>,
    /// The shell process. Not used yet, but we keep its handle.
    _child: Box<dyn Child + Send + Sync>,

    /// The cell under the mouse pointer, as (row, col).
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
}

impl App {
    /// Starts the shell and the reader thread. The window comes later, in `resumed`.
    /// `proxy` is how the reader thread wakes up the event loop.
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> anyhow::Result<Self> {
        let (master, child) = pty::spawn_shell(ROWS as u16, COLS as u16)?;
        let reader = master.try_clone_reader()?; // shell output comes out here
        // Our keys go in here. At start, ConPTY asks "where is the cursor?" (`ESC[6n`)
        // and shows nothing until the answer comes back through it (see `UserEvent::Reply`).
        let writer = master.take_writer()?;

        let terminal = Arc::new(Mutex::new(Terminal::new(ROWS, COLS)));
        spawn_reader(reader, terminal.clone(), proxy);

        Ok(App {
            window: None,
            renderer: None,
            terminal,
            writer,
            modifiers: ModifiersState::empty(),
            master,
            _child: child,

            mouse_cell: (0, 0),
            selection: None,
            selecting: false,

            cursor_on: true,
            next_blink: Instant::now() + BLINK_INTERVAL,

            scroll_rest: 0.0,
        })
    }

    /// Sends bytes to the shell, as if they were typed.
    fn send_to_shell(&mut self, bytes: &[u8]) {
        // Anything sent to the shell can change the screen, so an old mouse
        // selection would point at the wrong text: remove it.
        if self.selection.take().is_some() {
            self.request_redraw();
        }
        // If the shell is gone, the reader thread will report PtyExited.
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    /// Shows the cursor and starts the blink over, so it doesn't vanish while you type.
    fn restart_blink(&mut self) {
        self.cursor_on = true;
        self.next_blink = Instant::now() + BLINK_INTERVAL;
        self.request_redraw();
    }
}

/// Reads shell output on a background thread, feeds it to the parser,
/// then wakes the event loop so it can redraw.
fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
    terminal: Arc<Mutex<Terminal>>,
    proxy: EventLoopProxy<UserEvent>,
) {
    thread::spawn(move || {
        let mut parser = Parser::new();
        let mut buf = [0u8; 4096];
        loop {
            // `read` waits until the shell sends something, then gives `n` bytes.
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break, // the shell is gone
                Ok(n) => {
                    // The lock is held only while this chunk is parsed.
                    let replies = {
                        let mut terminal = terminal.lock().unwrap();
                        parser.advance(&mut *terminal, &buf[..n]);
                        // Take the answers out, leaving the list empty for the next chunk.
                        std::mem::take(&mut terminal.replies)
                    };
                    // The writer belongs to the main thread, so the answers go there.
                    // Err means the event loop has already shut down.
                    if !replies.is_empty() && proxy.send_event(UserEvent::Reply(replies)).is_err() {
                        return;
                    }
                    if proxy.send_event(UserEvent::PtyOutput).is_err() {
                        return;
                    }
                }
            }
        }
        let _ = proxy.send_event(UserEvent::PtyExited);
    });
}

/// Translates a key press into the bytes a terminal would send to the shell.
/// `None` means the key sends nothing (e.g. Shift alone).
fn key_to_bytes(key: &Key, text: Option<&str>, mods: ModifiersState) -> Option<Vec<u8>> {
    // Modifier number: 1 + Shift(1) + Alt(2) + Ctrl(4)
    // e.g. Ctrl+Left: m = 1 + 4 = 5, so it sends `ESC[1;5D`.
    let m = 1 + mods.shift_key() as u8 + 2 * mods.alt_key() as u8 + 4 * mods.control_key() as u8;

    // Arrows, Home and End: `\x1b[X` without modifiers, `\x1b[1;{m}X` with them.
    let with_mods = |letter: char| -> Option<Vec<u8>> {
        let seq = if m == 1 {
            format!("\x1b[{letter}")
        } else {
            format!("\x1b[1;{m}{letter}")
        };
        Some(seq.into_bytes())
    };

    // F1-F4: like arrows, but `\x1bOX` without modifiers (a leftover from the
    // PF1-PF4 keys of the VT100 terminal, which used `ESC O`).
    let f1_to_f4 = |letter: char| -> Option<Vec<u8>> {
        let seq = if m == 1 {
            format!("\x1bO{letter}")
        } else {
            format!("\x1b[1;{m}{letter}")
        };
        Some(seq.into_bytes())
    };

    // Insert, Delete, PageUp/Down, F5-F12: a number and `~`.
    // `\x1b[{n}~` without modifiers, `\x1b[{n};{m}~` with them, e.g. Ctrl+Delete: `\x1b[3;5~`.
    let tilde = |n: u8| -> Option<Vec<u8>> {
        let seq = if m == 1 {
            format!("\x1b[{n}~")
        } else {
            format!("\x1b[{n};{m}~")
        };
        Some(seq.into_bytes())
    };

    let bytes: &[u8] = match key {
        Key::Named(NamedKey::Enter) => b"\r", // carriage return, not "\n"
        Key::Named(NamedKey::Backspace) => b"\x7f", // the DEL character
        Key::Named(NamedKey::Tab) => b"\t",
        Key::Named(NamedKey::Escape) => b"\x1b",
        Key::Named(NamedKey::ArrowUp) => return with_mods('A'),
        Key::Named(NamedKey::ArrowDown) => return with_mods('B'),
        Key::Named(NamedKey::ArrowRight) => return with_mods('C'),
        Key::Named(NamedKey::ArrowLeft) => return with_mods('D'),
        Key::Named(NamedKey::Home) => return with_mods('H'),
        Key::Named(NamedKey::End) => return with_mods('F'),
        Key::Named(NamedKey::F1) => return f1_to_f4('P'),
        Key::Named(NamedKey::F2) => return f1_to_f4('Q'),
        Key::Named(NamedKey::F3) => return f1_to_f4('R'),
        Key::Named(NamedKey::F4) => return f1_to_f4('S'),
        Key::Named(NamedKey::Insert) => return tilde(2),
        Key::Named(NamedKey::Delete) => return tilde(3),
        Key::Named(NamedKey::PageUp) => return tilde(5),
        Key::Named(NamedKey::PageDown) => return tilde(6),
        // The gaps (16, 22) are from the VT220 keyboard, where they were
        // spaces between groups of keys. Every terminal kept the numbers.
        Key::Named(NamedKey::F5) => return tilde(15),
        Key::Named(NamedKey::F6) => return tilde(17),
        Key::Named(NamedKey::F7) => return tilde(18),
        Key::Named(NamedKey::F8) => return tilde(19),
        Key::Named(NamedKey::F9) => return tilde(20),
        Key::Named(NamedKey::F10) => return tilde(21),
        Key::Named(NamedKey::F11) => return tilde(23),
        Key::Named(NamedKey::F12) => return tilde(24),
        // Everything else (letters, digits, Cyrillic...): the typed text, as UTF-8 bytes.
        _ => return text.map(|t| t.as_bytes().to_vec()),
    };
    Some(bytes.to_vec())
}

// winit calls these methods when something happens; we never call them ourselves.
impl ApplicationHandler<UserEvent> for App {
    /// Called when the app starts. A window can only be created from here.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title("Kel");
        let window = Rc::new(event_loop.create_window(attrs).unwrap());
        // The cell size comes from the font, which the renderer loads, and the renderer
        // needs the window. So: window first, then renderer, then the 80x24 size.
        let renderer = Renderer::new(window.clone()).unwrap();
        let (cell_w, cell_h) = renderer.cell_size();
        let size = PhysicalSize::new((COLS * cell_w) as u32, (ROWS * cell_h) as u32);
        let _ = window.request_inner_size(size);
        self.renderer = Some(renderer);
        self.window = Some(window);
    }

    /// Our own events, sent by the reader thread.
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::PtyOutput => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            // Not `send_to_shell`: an answer isn't typing, so the mouse selection stays.
            UserEvent::Reply(bytes) => {
                let _ = self.writer.write_all(&bytes);
                let _ = self.writer.flush();
            }
            UserEvent::PtyExited => event_loop.exit(),
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
        if let Some(renderer) = &mut self.renderer {
            renderer.draw(
                &self.terminal.lock().unwrap().grid,
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
        // e.g. an 800x480 window with 10x20 cells: 80 cols, 24 rows.
        let size = window.inner_size();
        let (cell_w, cell_h) = renderer.cell_size();
        let cols = size.width as usize / cell_w;
        let rows = size.height as usize / cell_h;

        let mut terminal = self.terminal.lock().unwrap();
        let changed = (rows, cols) != (terminal.grid.rows(), terminal.grid.cols());
        if rows > 0 && cols > 0 && changed {
            terminal.grid.resize(rows, cols);
            let _ = self.master.resize(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    /// A key was pressed: handle our own shortcuts, or send the key to the shell.
    /// The order of the checks matters: the most specific shortcut comes first.
    /// Shortcuts use `physial_key` (the key's place on the keyboard), so they
    /// work with any layout, Cyrillic too.
    fn on_key(&mut self, event: KeyEvent) {
        self.restart_blink();
        let ctrl = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();

        // Ctrl+Shift+V: paste from the clipboard
        if ctrl && shift && event.physical_key == PhysicalKey::Code(KeyCode::KeyV) {
            if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                // The shell treats `\r` as Enter, so every line break becomes `\r`.
                let text = text.replace("\r\n", "\r").replace('\n', "\r");
                self.send_to_shell(text.as_bytes());
            }
            return;
        }

        // Ctrl+Shift+C: copy the mouse selection. Without one, the key goes on to the shell.
        if ctrl && shift && event.physical_key == PhysicalKey::Code(KeyCode::KeyC) {
            if let Some(selection) = self.selection {
                let (start, end) = selection.ordered();
                let text = self.terminal.lock().unwrap().grid.text_between(start, end);
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(text);
                }
                self.selection = None;
                self.request_redraw();
                return;
            }
        }

        // Ctrl+= / Ctrl+- / Ctrl+0: font bigger, smaller, back to the default
        if ctrl && let Some(renderer) = &mut self.renderer {
            let size = renderer.font_size();
            let new_size = match event.physical_key {
                PhysicalKey::Code(KeyCode::Equal) => Some(size + 1.0),
                PhysicalKey::Code(KeyCode::Minus) => Some(size - 1.0),
                PhysicalKey::Code(KeyCode::Digit0) => Some(FONT_SIZE),
                _ => None,
            };
            if let Some(new_size) = new_size {
                renderer.set_font_size(new_size);
                // The selection points at cells that move around when the grid changes size.
                self.selection = None;
                self.on_resize();
                self.request_redraw();
                return;
            }
        }

        // Ctrl+C: interrupt the running command.
        // Ctrl+Shift+C without a mouse selection also lands here, and PowerShell
        // then copies its own keyboard selection.
        if ctrl && event.physical_key == PhysicalKey::Code(KeyCode::KeyC) {
            self.send_to_shell(&[0x03]);
            return;
        }

        // Any other key: jump back to the bottom, then translate it to bytes for the shell
        self.terminal.lock().unwrap().grid.scroll_to_bottom();
        if let Some(bytes) = key_to_bytes(&event.logical_key, event.text.as_deref(), self.modifiers)
        {
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
            self.terminal.lock().unwrap().grid.scroll_view(whole as i32);
            self.request_redraw();
        }
    }

    /// The mouse moved: remember which cell it's over, and extend the selection while dragging.
    fn on_mouse_move(&mut self, position: PhysicalPosition<f64>) {
        let Some(renderer) = &self.renderer else {
            return;
        };
        let (cell_w, cell_h) = renderer.cell_size();
        // Pixels to cells, e.g. x=235, y=47 with 10x21 cells: col 23, row 2.
        let col = position.x as usize / cell_w;
        let row = position.y as usize / cell_h;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The bytes a named key sends, as a string, e.g. "\x1b[15~".
    fn send(key: NamedKey, mods: ModifiersState) -> String {
        let bytes = key_to_bytes(&Key::Named(key), None, mods).unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn f1_to_f4() {
        assert_eq!(send(NamedKey::F1, ModifiersState::empty()), "\x1bOP");
        assert_eq!(send(NamedKey::F4, ModifiersState::empty()), "\x1bOS");
        assert_eq!(send(NamedKey::F1, ModifiersState::SHIFT), "\x1b[1;2P");
    }

    #[test]
    fn tilde_keys() {
        assert_eq!(send(NamedKey::Insert, ModifiersState::empty()), "\x1b[2~");
        assert_eq!(send(NamedKey::F5, ModifiersState::empty()), "\x1b[15~");
        assert_eq!(send(NamedKey::F12, ModifiersState::empty()), "\x1b[24~");
        assert_eq!(send(NamedKey::Delete, ModifiersState::CONTROL), "\x1b[3;5~");
    }
}
