mod pty;
mod terminal;

use std::thread;
use std::time::Duration;
use vte::Parser;

use pty::spawn_shell;
use terminal::TerminalHandler;

fn main() {
    let session = spawn_shell();
    let mut reader = session.reader;
    let master = session.master;

    let command_writer = session.writer.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        {
            let mut w = command_writer.lock().unwrap();
            w.write_all(b"echo hello from kel\r\n").unwrap();
            w.flush().unwrap();
        }
        thread::sleep(Duration::from_millis(500));
        {
            let mut w = command_writer.lock().unwrap();
            w.write_all(
                b"cd C:\\Users\\Public\\Main\\workplace\\major-projects\\pare-app && dir\r\n",
            )
            .unwrap();
            w.flush().unwrap();
        }
        thread::sleep(Duration::from_millis(500));
        {
            let mut w = command_writer.lock().unwrap();
            w.write_all(b"exit\r\n").unwrap();
            w.flush().unwrap();
        }
    });

    let mut child = session.child;
    thread::spawn(move || {
        let status = child.wait().unwrap();
        drop(master);
        println!("\n[supervisor] shell exited, status: {:?}", status);
    });

    let mut parser = Parser::new();
    let mut handler = TerminalHandler {
        writer: session.writer.clone(),
        grid: terminal::Grid::new(24, 80),
    };

    let mut buf = [0u8; 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => parser.advance(&mut handler, &buf[..n]),
            Err(_) => break,
        }
    }

    println!("{}", handler.grid.render());
    println!("[main] reader loop finished cleanly");
}
