use std::io::Write;
use std::sync::{Arc, Mutex};
use vte::{Params, Perform};

pub struct TerminalHandler {
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub cursor_col: usize,
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
            'B' | 'e' | 'H' | 'f' => {
                println!();
                self.cursor_col = 0;
            }
            _ => {}
        }
    }
}
