//! One tab: a shell in its own pseudo-terminal, its screen, and two threads.
//!
//! - The **reader thread** waits for the shell's output, parses it into the grid,
//!   and sends `UserEvent::PtyOutput` so the main thread redraws.
//! - The **waiter thread** waits for the shell to end (`exit`, or the tab closed)
//!   and sends `UserEvent::PtyExited`. On Windows the output doesn't end when the
//!   shell does (ConPTY keeps it open), so only this thread can tell.
//!
//! The events carry the tab's id, not its place in the list: places change when
//! a tab before it closes, the id stays.
//!
//! The reader thread and the main thread both need the `Terminal`, so it lives in an
//! `Arc<Mutex<..>>`: `Arc` lets both threads own it, `Mutex` lets only one use it at a
//! time. Lock it, use it, let the lock go. Locking it twice at once freezes the app.

use crate::app::UserEvent;
use crate::pty;
use crate::term::Terminal;
use portable_pty::{Child, ChildKiller, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use vte::Parser;
use winit::event_loop::EventLoopProxy;

pub type TabId = u64;

/// After a resize, ConPTY sends the whole screen again: measured, 1 ms after it,
/// ~270 bytes. Output this soon after a resize is that copy, not anything new.
const RESIZE_ECHO: Duration = Duration::from_millis(300);

pub struct Tab {
    pub id: TabId,
    /// The name in the tab bar: the program's title, made shorter (see `short_title`),
    /// or the shell's name if the program gave none.
    pub title: String,
    /// The shell's name, e.g. "powershell".
    shell_name: String,
    /// New output since the tab was last on screen: a dot in the tab bar.
    pub activity: bool,
    /// When the grid last changed size (see `RESIZE_ECHO`).
    resized_at: Instant,
    /// The screen state, shared with the reader thread.
    pub terminal: Arc<Mutex<Terminal>>,
    /// Bytes written here reach the shell as if they were typed.
    writer: Box<dyn Write + Send>,
    /// Our end of the PTY; used to tell the shell about a new size.
    master: Box<dyn MasterPty + Send>,
    /// Ends the shell. The waiter thread holds the shell itself, to wait for it.
    killer: Box<dyn ChildKiller + Send + Sync>,
}

impl Tab {
    /// Starts `shell` in a new tab, as big as `terminal`'s grid. `terminal` can
    /// already hold text (Kel's error messages at start); the shell starts below it.
    pub fn spawn(
        id: TabId,
        shell: &str,
        terminal: Terminal,
        proxy: EventLoopProxy<UserEvent>,
    ) -> anyhow::Result<Tab> {
        let (rows, cols) = (terminal.grid.rows() as u16, terminal.grid.cols() as u16);
        let (master, child) = pty::spawn_shell(shell, rows, cols)?;
        let reader = master.try_clone_reader()?; // shell output comes out here
        // Our keys go in here. At start, ConPTY asks "where is the cursor?" (`ESC[6n`)
        // and shows nothing until the answer comes back through it (see `UserEvent::Reply`).
        let writer = master.take_writer()?;
        let killer = child.clone_killer();

        let terminal = Arc::new(Mutex::new(terminal));
        spawn_reader(id, reader, terminal.clone(), proxy.clone());
        spawn_waiter(id, child, proxy);
        let shell_name = short_title(shell);
        Ok(Tab {
            id,
            title: shell_name.clone(),
            shell_name,
            activity: false,
            resized_at: Instant::now(),
            terminal,
            writer,
            master,
            killer,
        })
    }

    /// Sends bytes to the shell, as if they were typed.
    pub fn write(&mut self, bytes: &[u8]) {
        // If the shell is gone, the waiter thread reports PtyExited.
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    /// Fits the grid to `rows` x `cols` cells and tells the shell, which then redraws.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        let mut terminal = self.terminal.lock().unwrap();
        if (rows, cols) == (terminal.grid.rows(), terminal.grid.cols()) {
            return;
        }
        terminal.grid.resize(rows, cols);
        self.resized_at = Instant::now();
        let _ = self.master.resize(PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    /// Takes the title the program sent, if it changed. Returns `true` if the
    /// name in the tab bar is now different.
    pub fn refresh_title(&mut self) -> bool {
        let title = match self.terminal.lock().unwrap().title.as_deref() {
            Some(title) if !title.trim().is_empty() => short_title(title),
            // No title, or an empty one: back to the shell's name.
            _ => self.shell_name.clone(),
        };
        if title == self.title {
            return false;
        }
        self.title = title;
        true
    }

    /// The shell wrote something while the tab is not on screen: light the dot.
    /// Returns `true` if it just lit, so the tab bar needs a redraw.
    pub fn mark_activity(&mut self) -> bool {
        if self.activity || self.resized_at.elapsed() < RESIZE_ECHO {
            return false;
        }
        self.activity = true;
        true
    }

    /// Ends the shell. The tab itself goes away when the waiter thread reports it.
    pub fn kill(&mut self) {
        // On Windows portable-pty reports an error even when this works (it checks
        // the result the wrong way round), so the result says nothing: ignore it.
        let _ = self.killer.kill();
    }
}

impl Drop for Tab {
    /// A tab that goes away takes its shell with it, also when Kel closes.
    fn drop(&mut self) {
        self.kill();
    }
}

/// A title made to fit in a tab: a program's path becomes its name, the rest stays.
/// - `C:\WINDOWS\...\powershell.exe` → `powershell`
/// - `C:\WINDOWS\system32\cmd.exe - ping localhost` → `cmd - ping localhost`
/// - `Cargo.toml - Nvim` → as it is
fn short_title(title: &str) -> String {
    // The path ends at ".exe", in any case ("CMD.EXE"). Lowercase has the same
    // byte length for these letters, so a position in it is one in `title` too.
    match title.to_ascii_lowercase().find(".exe") {
        Some(end) => {
            let path = &title[..end];
            // The name is after the last '\' or '/'.
            let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
            format!("{name}{}", &title[end + 4..])
        }
        None => title.to_string(),
    }
}

/// Reads shell output on a background thread, feeds it to the parser,
/// then wakes the event loop so it can redraw.
fn spawn_reader(
    id: TabId,
    mut reader: Box<dyn Read + Send>,
    terminal: Arc<Mutex<Terminal>>,
    proxy: EventLoopProxy<UserEvent>,
) {
    thread::spawn(move || {
        let mut parser = Parser::new();
        let mut buf = [0u8; 4096];
        // `read` waits until the shell sends something, then gives `n` bytes.
        // 0 or an error: the PTY was closed, the tab is gone.
        while let Ok(n @ 1..) = reader.read(&mut buf) {
            // The lock is held only while this chunk is parsed.
            let replies = {
                let mut terminal = terminal.lock().unwrap();
                parser.advance(&mut *terminal, &buf[..n]);
                // Take the answers out, leaving the list empty for the next chunk.
                std::mem::take(&mut terminal.replies)
            };
            // The writer belongs to the main thread, so the answers go there.
            // Err means the event loop has already shut down.
            if !replies.is_empty() && proxy.send_event(UserEvent::Reply(id, replies)).is_err() {
                return;
            }
            if proxy.send_event(UserEvent::PtyOutput(id)).is_err() {
                return;
            }
        }
    });
}

/// Waits on a background thread for the shell to end, then tells the event loop.
fn spawn_waiter(
    id: TabId,
    mut child: Box<dyn Child + Send + Sync>,
    proxy: EventLoopProxy<UserEvent>,
) {
    thread::spawn(move || {
        let _ = child.wait();
        let _ = proxy.send_event(UserEvent::PtyExited(id));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_titles() {
        let powershell = r"C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe";
        assert_eq!(short_title(powershell), "powershell");
        assert_eq!(short_title("powershell.exe"), "powershell");
        assert_eq!(
            short_title(r"C:\Program Files\PowerShell\7\pwsh.exe"),
            "pwsh"
        );
        assert_eq!(
            short_title(r"C:\WINDOWS\system32\CMD.EXE - ping localhost"),
            "CMD - ping localhost"
        );
        assert_eq!(short_title("Cargo.toml - Nvim"), "Cargo.toml - Nvim");
    }
}
