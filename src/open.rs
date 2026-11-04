//! Opening things the way the system does: a web page in the default browser,
//! the config in the text editor, a file shown in the file manager. Each system
//! has its own program for it:
//!
//! | | Windows | macOS | Linux |
//! |---|---|---|---|
//! | a web page | `rundll32 url.dll` | `open` | `xdg-open` |
//! | a text file | `explorer` | `open -t` | `xdg-open` |
//! | a folder | `explorer` | `open` | `xdg-open` |
//! | a file, shown | `explorer /select,` | `open -R` | its folder |
//!
//! Nothing here runs the file itself: a `.ps1` or a `.sh` opens as text, or
//! in the file manager.

use std::io;
use std::path::Path;
use std::process::Command;

/// Opens a web page in the default browser, as a click on a link in another
/// program would. Windows: `rundll32` and not `explorer`, which cuts its
/// argument at commas, and links have them.
pub fn page(url: &str) -> io::Result<()> {
    if cfg!(windows) {
        let mut command = Command::new("rundll32");
        command.args(["url.dll,FileProtocolHandler", url]);
        run(command)
    } else {
        let mut command = opener();
        command.arg(url);
        run(command)
    }
}

/// Opens a text file (the config) in the editor the system has for it.
/// macOS: `open -t`, the text editor, since a `.toml` has no program of its own there.
pub fn text_file(path: &Path) -> io::Result<()> {
    let mut command = opener();
    if cfg!(target_os = "macos") {
        command.arg("-t");
    }
    command.arg(path);
    run(command)
}

/// Opens a folder in the file manager.
pub fn folder(path: &Path) -> io::Result<()> {
    let mut command = opener();
    command.arg(path);
    run(command)
}

/// Shows a file in the file manager: its folder open, the file chosen.
#[cfg(windows)]
pub fn reveal(path: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    // `/select,"C:\a, b.txt"`: in quotes, so a comma in the name stays.
    let mut command = Command::new("explorer");
    command.raw_arg(format!("/select,\"{}\"", path.display()));
    run(command)
}

/// Shows a file in Finder: its folder open, the file chosen.
#[cfg(target_os = "macos")]
pub fn reveal(path: &Path) -> io::Result<()> {
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    run(command)
}

/// Linux has no one way to choose a file in the file manager (each has its
/// own): its folder opens.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn reveal(path: &Path) -> io::Result<()> {
    folder(path.parent().unwrap_or(path))
}

/// The system's program that opens a thing as a double-click on it would.
fn opener() -> Command {
    Command::new(if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    })
}

/// Starts `command` without waiting for it. A thread waits instead: on Linux
/// and macOS a program that ended stays in the process list (a "zombie") until
/// the one that started it asks how it ended.
fn run(mut command: Command) -> io::Result<()> {
    let mut child = command.spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}
