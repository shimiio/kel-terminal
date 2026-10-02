mod grid;
mod pty;
mod terminal;

use grid::Grid;
use std::io::{Read, Write, stdin};
use std::sync::{Arc, Mutex};
use std::thread;
use terminal::Terminal;
use vte::Parser;

fn main() -> anyhow::Result<()> {
    let (master, _child) = pty::spawn_shell()?;
    let mut reader = master.try_clone_reader()?;
    let mut writer = master.take_writer()?;

    let mut parser = Parser::new();
    let performer = Arc::new(Mutex::new(Terminal {
        grid: Grid::new(24, 80),
    }));
    let performer_clone = performer.clone();

    // answer the ConPTY cursor position handshake
    writer.write_all(b"\x1b[1;1R")?;
    writer.flush()?;

    // reader thread
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut p = performer_clone.lock().unwrap();
                    parser.advance(&mut *p, &buf[..n]);
                    print!("\x1b[2J\x1b[H");
                    print!("{}", p.grid.render());
                    std::io::stdout().flush().unwrap();
                }
            }
        }
    });

    // main thread
    loop {
        let mut line = String::new();
        stdin().read_line(&mut line)?;
        writer.write_all(line.trim_end().as_bytes())?;
        writer.write_all(b"\r")?;
        writer.flush()?;
    }
}
