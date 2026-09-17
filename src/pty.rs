use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

pub struct PtySession {
    pub reader: Box<dyn Read + Send>,
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn Child + Send + Sync>,
}

pub fn spawn_shell() -> PtySession {
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
    let child = pair
        .slave
        .spawn_command(cmd)
        .expect("failed to spawn shell");
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .expect("failed to clone reader");
    let writer = pair.master.take_writer().expect("failed to take writer");

    PtySession {
        reader,
        writer: Arc::new(Mutex::new(writer)),
        master: pair.master,
        child,
    }
}
