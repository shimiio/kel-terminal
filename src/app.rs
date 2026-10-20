use crate::pty;
use crate::renderer::{CELL_HEIGHT, CELL_WIDTH, Renderer};
use crate::term::Terminal;
use portable_pty::{Child, MasterPty, PtySize};
use std::io::{Read, Write};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use vte::Parser;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{Window, WindowId};

const ROWS: usize = 24;
const COLS: usize = 80;

/// The selected area, from where the mouse was pressed to where it is now. (row, col)
#[derive(Clone, Copy, Debug)]
pub struct Selection {
    pub start: (usize, usize),
    pub end: (usize, usize),
}

/// Custom events sent from other threads to wake up the event loop.
#[derive(Debug)]
pub enum UserEvent {
    /// The reader thread parsed new shell output into the grid.
    PtyOutput,
    /// The shell closed its output.
    PtyExited,
}

pub struct App {
    window: Option<Rc<Window>>,
    renderer: Option<Renderer>,
    terminal: Arc<Mutex<Terminal>>,
    modifiers: ModifiersState,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    _child: Box<dyn Child + Send + Sync>,

    mouse_cell: (usize, usize),
    selection: Option<Selection>,
    selecting: bool,
}

impl App {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> anyhow::Result<Self> {
        let (master, child) = pty::spawn_shell(ROWS as u16, COLS as u16)?;
        let reader = master.try_clone_reader()?;
        let mut writer = master.take_writer()?;

        // answer the ConPTY cursor position handshake
        writer.write_all(b"\x1b[1;1R")?;
        writer.flush()?;

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
        })
    }

    fn send_to_shell(&mut self, bytes: &[u8]) {
        // If the shell is gone, the reader thread will report PtyExited.
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
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
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    parser.advance(&mut *terminal.lock().unwrap(), &buf[..n]);
                    // Err means the event loop has already shut down.
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
fn key_to_bytes(key: &Key, text: Option<&str>, mods: ModifiersState) -> Option<Vec<u8>> {
    // Modifier number: 1 + Shift(1) + Alt(2) + Ctrl(4)
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

    let bytes: &[u8] = match key {
        Key::Named(NamedKey::Enter) => b"\r",
        Key::Named(NamedKey::Backspace) => b"\x7f",
        Key::Named(NamedKey::Delete) => b"\x1b[3~",
        Key::Named(NamedKey::Tab) => b"\t",
        Key::Named(NamedKey::Escape) => b"\x1b",
        Key::Named(NamedKey::PageUp) => b"\x1b[5",
        Key::Named(NamedKey::PageDown) => b"\x1b[6~",
        Key::Named(NamedKey::ArrowUp) => return with_mods('A'),
        Key::Named(NamedKey::ArrowDown) => return with_mods('B'),
        Key::Named(NamedKey::ArrowRight) => return with_mods('C'),
        Key::Named(NamedKey::ArrowLeft) => return with_mods('D'),
        Key::Named(NamedKey::Home) => return with_mods('H'),
        Key::Named(NamedKey::End) => return with_mods('F'),
        _ => return text.map(|t| t.as_bytes().to_vec()),
    };
    Some(bytes.to_vec())
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let size = PhysicalSize::new((COLS * CELL_WIDTH) as u32, (ROWS * CELL_HEIGHT) as u32);
        let attrs = Window::default_attributes()
            .with_title("Kel")
            .with_inner_size(size);
        let window = Rc::new(event_loop.create_window(attrs).unwrap());
        let renderer = Renderer::new(window.clone()).unwrap();
        self.renderer = Some(renderer);
        self.window = Some(window);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::PtyOutput => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            UserEvent::PtyExited => event_loop.exit(),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.terminal.lock().unwrap().grid.scroll_to_bottom();

                // control + C
                if self.modifiers.control_key()
                    && event.physical_key == PhysicalKey::Code(KeyCode::KeyC)
                {
                    self.send_to_shell(&[0x03]);
                    return;
                }

                // control + shift + V - paste
                if self.modifiers.control_key()
                    && self.modifiers.shift_key()
                    && event.physical_key == PhysicalKey::Code(KeyCode::KeyV)
                {
                    if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                        let text = text.replace("\r\n", "\r").replace('\n', "\r");
                        self.send_to_shell(text.as_bytes());
                    }
                    return;
                }

                //
                if let Some(bytes) =
                    key_to_bytes(&event.logical_key, event.text.as_deref(), self.modifiers)
                {
                    self.send_to_shell(&bytes);
                }
            }
            WindowEvent::Resized(_) => {
                if let (Some(renderer), Some(window)) = (&self.renderer, &self.window) {
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
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.draw(&self.terminal.lock().unwrap().grid);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let MouseScrollDelta::LineDelta(_, y) = delta {
                    let lines = (y * 3.0) as i32;
                    let mut terminal = self.terminal.lock().unwrap();
                    terminal.grid.scroll_view(lines);
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let col = position.x as usize / CELL_WIDTH;
                let row = position.y as usize / CELL_HEIGHT;
                self.mouse_cell = (row, col);

                if self.selecting {
                    if let Some(selection) = &mut self.selection {
                        selection.end = (row, col);
                        println!("{:?}", selection);
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => {
                    self.selection = Some(Selection {
                        start: self.mouse_cell,
                        end: self.mouse_cell,
                    });
                    self.selecting = true;
                }
                ElementState::Released => self.selecting = false,
            },
            _ => {}
        }
    }
}
