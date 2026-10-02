mod app;
mod grid;
mod pty;
mod terminal;

use app::KelApp;
use grid::Grid;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use terminal::Terminal;
use vte::Parser;

fn main() -> anyhow::Result<()> {
    // core
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

    // gui
    eframe::run_native(
        "Kel",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();

            // reader thread
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            {
                                let mut p = performer_clone.lock().unwrap();
                                parser.advance(&mut *p, &buf[..n]);
                            }
                            ctx.request_repaint();
                        }
                    }
                }
            });
            Ok(Box::new(KelApp::new(performer)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}
