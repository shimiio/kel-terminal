use crate::pty;
use crate::renderer::Renderer;
use crate::term::Terminal;
use portable_pty::{Child, MasterPty, PtySize};
use std::io::{Read, Write};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use vte::Parser;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const ROWS: usize = 24;
const COLS: usize = 80;

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
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    _child: Box<dyn Child + Send + Sync>,
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
            master,
            _child: child,
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
fn key_to_bytes(key: &Key, text: Option<&str>) -> Option<Vec<u8>> {
    let bytes: &[u8] = match key {
        Key::Named(NamedKey::Enter) => b"\r",
        Key::Named(NamedKey::Backspace) => b"\x7f",
        Key::Named(NamedKey::Tab) => b"\t",
        Key::Named(NamedKey::Escape) => b"\x1b",
        Key::Named(NamedKey::ArrowUp) => b"\x1b[A",
        Key::Named(NamedKey::ArrowDown) => b"\x1b[B",
        Key::Named(NamedKey::ArrowRight) => b"\x1b[C",
        Key::Named(NamedKey::ArrowLeft) => b"\x1b[D",
        _ => return text.map(|t| t.as_bytes().to_vec()),
    };
    Some(bytes.to_vec())
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title("Kel");
        let window = Rc::new(event_loop.create_window(attrs).unwrap());
        let renderer = Renderer::new(window.clone()).unwrap();
        // Make the window exactly as big as the grid.
        let (cell_w, cell_h) = renderer.cell_size();
        let _ = window.request_inner_size(PhysicalSize::new(
            (COLS * cell_w) as u32,
            (ROWS * cell_h) as u32,
        ));
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
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(bytes) = key_to_bytes(&event.logical_key, event.text.as_deref()) {
                    self.send_to_shell(&bytes);
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &self.renderer {
                    let (cell_w, cell_h) = renderer.cell_size();
                    let cols = size.width as usize / cell_w;
                    let rows = size.height as usize / cell_h;
                    if rows > 0 && cols > 0 {
                        self.terminal.lock().unwrap().grid.resize(rows, cols);
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
            _ => {}
        }
    }
}
