use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use vte::{Params, Parser, Perform};

struct TerminalHandler {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    cursor_col: usize,
}

impl Perform for TerminalHandler {
    fn print(&mut self, c: char) {
        print!("{}", c);
        self.cursor_col += 1;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => {
                print!("\r");
                self.cursor_col = 0;
            }
            b'\n' => {
                println!();
                self.cursor_col = 0;
            }
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        match action {
            'n' => {
                let is_cpr_request = params.iter().flatten().any(|&p| p == 6);
                if is_cpr_request {
                    let mut w = self.writer.lock().unwrap();
                    w.write_all(b"\x1b[1;1R").unwrap();
                    w.flush().unwrap();
                }
            }
            'B' | 'e' => {
                println!();
                self.cursor_col = 0;
            }
            'H' | 'f' => {
                println!();
                self.cursor_col = 0;
            }
            _ => {}
        }
    }
}

fn main() {
    let pty_system = native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("failed to create PTY");

    let shell = if cfg!(windows) { "cmd.exe" } else { "bash" };
    let cmd = CommandBuilder::new(shell);

    // spawn shell
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .expect("failed to spawn shell");
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .expect("failed to clone reader");
    let writer = pair.master.take_writer().expect("failed to take writer");
    let writer = Arc::new(Mutex::new(writer));

    let master = pair.master;

    let command_writer = Arc::clone(&writer);
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
            w.write_all(b"exit\r\n").unwrap();
            w.flush().unwrap();
        }
    });

    thread::spawn(move || {
        let status = child.wait().unwrap();
        drop(master);
        println!("\n[supervisor] shell exited, status: {:?}", status);
    });

    let mut parser = Parser::new();
    let mut handler = TerminalHandler {
        writer: Arc::clone(&writer),
        cursor_col: 0,
    };

    let mut buf = [0u8; 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => parser.advance(&mut handler, &buf[..n]),
            Err(_) => break,
        }
    }

    println!("[main] reader loop finished cleanly");
}
