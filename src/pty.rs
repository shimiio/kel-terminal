//! Starting the shell.
//!
//! A pseudo-terminal (PTY; ConPTY on Windows) makes PowerShell believe it runs
//! in a real console window. It has two ends:
//! - the **slave** end is given to the shell as its console;
//! - the **master** end is ours: we read the shell's output from it and write keys into it.

use crate::shell_integration;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::path::Path;

/// Starts `shell` (e.g. "powershell.exe") in a new PTY of `rows` x `cols` cells,
/// in the folder `cwd` if it's given and still there (else where Kel runs).
/// Returns our (master) end and the shell process.
pub fn spawn_shell(
    shell: &str,
    cwd: Option<&Path>,
    rows: u16,
    cols: u16,
) -> anyhow::Result<(Box<dyn MasterPty + Send>, Box<dyn Child + Send + Sync>)> {
    let pair = native_pty_system().openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new(shell);
    shell_integration::add(&mut cmd, shell);
    // A deleted folder would make the start fail.
    if let Some(cwd) = cwd.filter(|cwd| cwd.is_dir()) {
        cmd.cwd(cwd);
    }
    let child = pair.slave.spawn_command(cmd)?;
    // The shell holds the slave end now. We drop our copy, so that when the
    // shell exits, reading from the master ends instead of waiting forever.
    drop(pair.slave);
    Ok((pair.master, child))
}
