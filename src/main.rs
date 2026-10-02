mod grid;
mod pty;
mod terminal;

use grid::Grid;
use std::io::{Read, Write, stdin};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use terminal::Terminal;
use vte::Parser;

fn main() -> anyhow::Result<()> {
    let (master, mut child) = pty::spawn_shell()?;
    let mut reader = master.try_clone_reader()?;
    let mut writer = master.take_writer()?;

    let mut parser = Parser::new();
    let mut performer = Terminal {
        grid: Grid::new(24, 80),
    };

    // get output
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // flush
    writer.write_all(b"\x1b[1;1R")?; // answer the cursor request
    writer.flush()?;

    // real input
    let mut line = String::new();
    stdin().read_line(&mut line)?;
    writer.write_all(line.as_bytes())?;
    writer.flush()?;

    while let Ok(chunk) = rx.recv_timeout(Duration::from_secs(1)) {
        parser.advance(&mut performer, &chunk);
    }

    child.kill()?;
    println!("{}", performer.grid.render());
    Ok(())
}
