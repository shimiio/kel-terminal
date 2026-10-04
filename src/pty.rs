use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

pub fn spawn_shell() -> anyhow::Result<(Box<dyn MasterPty + Send>, Box<dyn Child + Send + Sync>)> {
    let pair = native_pty_system().openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let cmd = CommandBuilder::new("powershell.exe");
    let child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);
    Ok((pair.master, child))
}
