// Temporary: does text that's in the grid before a shell starts (a tab's output
// from last time) stay whole under ConPTY? cargo run --example probe_restore
//
// For each shell and each amount of old text: the shell starts below it, a
// command runs, the window is resized, the screen is cleared. After each step:
// how many of the old lines are still there, and the screen.
use kel_terminal::pty;
use kel_terminal::term::{Line, Terminal};
use portable_pty::PtySize;
use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};
use vte::Parser;

struct Probe {
    terminal: Terminal,
    parser: Parser,
    chunks: Receiver<Vec<u8>>,
    writer: Box<dyn Write + Send>,
    old: usize,
}

impl Probe {
    /// Output into the terminal for `time`, the answers back, as `tab::pump` does.
    /// The bytes that came, to look at.
    fn run(&mut self, time: Duration) -> Vec<u8> {
        let end = Instant::now() + time;
        let mut all = Vec::new();
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            let Ok(chunk) = self.chunks.recv_timeout(left) else {
                break;
            };
            self.parser.advance(&mut self.terminal, &chunk);
            self.terminal.grid.settle();
            all.extend_from_slice(&chunk);
            let replies = std::mem::take(&mut self.terminal.replies);
            if !replies.is_empty() {
                let _ = self.writer.write_all(&replies);
                let _ = self.writer.flush();
            }
        }
        all
    }

    fn report(&self, step: &str, bytes: &[u8]) {
        let grid = &self.terminal.grid;
        let text = |line: &Line| line.cells.iter().map(|c| c.ch).collect::<String>();
        let above: Vec<String> = grid.scrollback().iter().map(text).collect();
        let screen = grid.render();
        let is_old = |line: &&str| line.starts_with("old line ");
        let kept = above.iter().map(String::as_str).filter(is_old).count()
            + screen.lines().filter(is_old).count();
        println!("--- {step}");
        println!(
            "    old lines kept: {kept} of {}; scrollback rows: {}; cursor: {:?}",
            self.old,
            above.len(),
            grid.cursor()
        );
        if !bytes.is_empty() {
            let shown = String::from_utf8_lossy(&bytes[..bytes.len().min(700)]).into_owned();
            println!("    bytes ({}): {shown:?}", bytes.len());
        }
        // The view, as the renderer reads it: `>` is the cursor's row.
        let cursor = grid.cursor().0 + grid.above();
        for row in 0..grid.rows() {
            let cells = (0..grid.cols()).map(|col| grid.visible_cell(row, col).ch);
            let text: String = cells.collect();
            let mark = if row == cursor { '>' } else { '|' };
            println!("    {mark} {}", text.trim_end());
        }
    }
}

fn probe(shell: &str, old: usize) -> anyhow::Result<()> {
    println!("\n===== {shell}, {old} old lines =====");
    let (rows, cols) = (24, 80);
    let mut terminal = Terminal::new(rows, cols);
    let mut parser = Parser::new();
    let mut last = String::new();
    for i in 1..=old {
        // `KEL_PROBE_LONG`: 90 letters, 2 rows at 80 columns, 1 at 100.
        let tail = match std::env::var_os("KEL_PROBE_LONG") {
            Some(_) => "x".repeat(80),
            None => String::new(),
        };
        last += &format!("old line {i} {tail}\r\n");
    }
    // `KEL_PROBE_SCREEN`: left on the screen, the way that broke (see
    // `Grid::start_below`); else as Kel does it, above the screen.
    match std::env::var_os("KEL_PROBE_SCREEN") {
        Some(_) => parser.advance(&mut terminal, last.as_bytes()),
        None => terminal.show_last_time(&last, "14 Nov 18:32"),
    }

    let history = std::env::temp_dir().join("kel-probe-history.txt");
    let (master, mut child) =
        pty::spawn_shell(shell, None, Some(&history), false, rows as u16, cols as u16)?;
    let mut reader = master.try_clone_reader()?;
    let writer = master.take_writer()?;
    let (send, chunks) = channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n @ 1..) = reader.read(&mut buf) {
            if send.send(buf[..n].to_vec()).is_err() {
                return;
            }
        }
    });
    let mut probe = Probe {
        terminal,
        parser,
        chunks,
        writer,
        old,
    };

    probe.report("before the shell", &[]);
    let bytes = probe.run(Duration::from_secs(6));
    probe.report("the shell started", &bytes);

    // `KEL_PROBE`: one command while the old text is still on the screen, e.g.
    // a clear, and nothing else.
    if let Ok(command) = std::env::var("KEL_PROBE") {
        probe.writer.write_all(format!("{command}\r").as_bytes())?;
        let bytes = probe.run(Duration::from_secs(3));
        probe.report(&command, &bytes);
        let _ = child.kill();
        return Ok(());
    }

    probe.writer.write_all(b"echo hi\r")?;
    let bytes = probe.run(Duration::from_secs(2));
    probe.report("echo hi", &bytes);

    // A program that prints more than a screen.
    probe.writer.write_all(b"for %i in (1 2 3) do @echo x\r")?;
    probe.run(Duration::from_secs(2));

    let (rows, cols) = (30, 100);
    probe.terminal.grid.resize(rows, cols);
    master.resize(PtySize {
        rows: rows as u16,
        cols: cols as u16,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let bytes = probe.run(Duration::from_secs(2));
    probe.report("resized to 30 x 100", &bytes);

    let (rows, cols) = (20, 60);
    probe.terminal.grid.resize(rows, cols);
    master.resize(PtySize {
        rows: rows as u16,
        cols: cols as u16,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let bytes = probe.run(Duration::from_secs(2));
    probe.report("resized to 20 x 60", &bytes);

    probe.writer.write_all(b"cls\r")?;
    let bytes = probe.run(Duration::from_secs(2));
    probe.report("cls", &bytes);

    let _ = child.kill();
    Ok(())
}

/// `KEL_PROBE_TIME`: how long a full scrollback takes to read and to write,
/// 10 000 lines of 100 letters in two colors.
fn time() {
    let line = format!("\x1b[32m{}\x1b[m {}\r\n", "g".repeat(50), "w".repeat(49));
    let last = line.repeat(10_000);
    let mut terminal = Terminal::new(50, 170);
    let start = Instant::now();
    terminal.show_last_time(&last, "14 Nov 18:32");
    let read = start.elapsed();
    let start = Instant::now();
    let output = terminal.grid.output(false);
    println!(
        "read {} KB in {read:?}, written {} KB in {:?}",
        last.len() / 1024,
        output.len() / 1024,
        start.elapsed()
    );
}

fn main() -> anyhow::Result<()> {
    if std::env::var_os("KEL_PROBE_TIME").is_some() {
        time();
        return Ok(());
    }
    let shells: Vec<String> = std::env::args().skip(1).collect();
    let shells = if shells.is_empty() {
        vec!["powershell.exe -NoLogo".to_string(), "cmd.exe".to_string()]
    } else {
        shells
    };
    for shell in &shells {
        for old in [5, 60] {
            probe(shell, old)?;
        }
    }
    Ok(())
}
