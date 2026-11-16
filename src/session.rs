//! A project's tabs come back with their output from last time.
//!
//! A project's file says which tabs to open (see `project`), and their
//! commands start again. What the tabs showed is kept here, a file per tab:
//!
//! ```text
//! %APPDATA%\kel\sessions\myapp\tab-server.txt   the tab named "server"
//! %APPDATA%\kel\sessions\myapp\2.txt            the second tab, without a name
//! ```
//!
//! - Written when the project closes (or the window, with it open), with the
//!   project's file: the tabs there then, no others.
//! - Read when the project opens: the text goes into the tab's grid before its
//!   shell starts (see `Terminal::show_last_time`), and the prompt comes below.
//! - A file is what a program would print (`Grid::output`): the letters and
//!   `ESC[...m` for their colors. No program comes back, only what it wrote.
//! - It comes back a little fainter than what's new, and a line across the
//!   window ends it, with the day and the time the file was written. The
//!   prompt the shell waited at isn't kept: a new one comes below the line.
//! - A tab with a name is found by it, as its history of commands is; one
//!   without, by its place in the bar, which the project's file keeps.
//! - Home's tabs have none: home starts empty, each time.
//! - `restore_output = false` in the config: nothing is kept, and what was
//!   goes when the project closes next. The files are plain text, with all a
//!   tab showed, a password printed too.
//!
//! Nothing here is an error to show: a tab whose output can't be kept or read
//! starts empty, as it always did.

use crate::clock;
use crate::config;
use crate::history::file_name;
use std::path::{Path, PathBuf};

/// A tab of a project as it closes: its name, and its output (`Grid::output`).
pub struct Output {
    pub name: Option<String>,
    pub text: String,
}

/// Keeps the output of `project`'s tabs, left to right, in place of what was
/// kept before: a tab closed since has no file left.
pub fn save(project: &str, tabs: &[Output]) {
    if let Some(dir) = dir() {
        save_in(&dir, project, tabs);
    }
}

/// What was kept for a tab: its output, and when (`14 Nov 18:32`, the file's
/// time; empty if the system can't say).
pub struct Last {
    pub text: String,
    pub when: String,
}

/// The output kept for a tab of `project`: by its name, or by `place`, its
/// place in the bar from 1. `None`: none kept, the tab starts empty.
pub fn load(project: &str, name: Option<&str>, place: usize) -> Option<Last> {
    let file = file_in(&dir()?, project, name, place);
    let text = std::fs::read_to_string(&file).ok()?;
    let written = std::fs::metadata(&file).and_then(|file| file.modified());
    let when = written.ok().and_then(clock::day_and_time);
    (!text.is_empty()).then_some(Last {
        text,
        when: when.unwrap_or_default(),
    })
}

/// The project `old` is named `new` now: its tabs' output goes with it.
pub fn rename_project(old: &str, new: &str) {
    if let Some(dir) = dir() {
        let (from, to) = (dir.join(old), dir.join(new));
        if from.exists() && !to.exists() {
            let _ = std::fs::rename(from, to);
        }
    }
}

/// The project `name` is deleted, or keeps no output from now on: what its
/// tabs showed goes.
pub fn delete_project(name: &str) {
    if let Some(dir) = dir() {
        let _ = std::fs::remove_dir_all(dir.join(name));
    }
}

/// Where the output is: `sessions` in Kel's folder, a folder per project.
fn dir() -> Option<PathBuf> {
    Some(config::dir().ok()?.join("sessions"))
}

/// `save`, in `dir`.
fn save_in(dir: &Path, project: &str, tabs: &[Output]) {
    let folder = dir.join(project);
    let _ = std::fs::remove_dir_all(&folder);
    for (place, tab) in (1..).zip(tabs) {
        // A tab that showed nothing: no file, it starts empty.
        if tab.text.is_empty() || std::fs::create_dir_all(&folder).is_err() {
            continue;
        }
        let file = file_in(dir, project, tab.name.as_deref(), place);
        let _ = std::fs::write(file, &tab.text);
    }
}

/// The file of a tab of `project`, in `dir`: `tab-<name>.txt` (no number is
/// one), or `<place>.txt`.
fn file_in(dir: &Path, project: &str, name: Option<&str>, place: usize) -> PathBuf {
    let file = match name {
        Some(name) => format!("tab-{}.txt", file_name(name)),
        None => format!("{place}.txt"),
    };
    dir.join(project).join(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(name: Option<&str>, text: &str) -> Output {
        Output {
            name: name.map(str::to_string),
            text: text.to_string(),
        }
    }

    #[test]
    fn a_file_per_tab() {
        let dir = Path::new("sessions");
        let project = dir.join("myapp");
        assert_eq!(
            file_in(dir, "myapp", Some("server"), 1),
            project.join("tab-server.txt")
        );
        assert_eq!(file_in(dir, "myapp", None, 2), project.join("2.txt"));
        // A tab named like a place is still found by its name.
        assert_eq!(
            file_in(dir, "myapp", Some("2"), 1),
            project.join("tab-2.txt")
        );
    }

    #[test]
    fn the_tabs_there_now_and_no_others() {
        let dir = std::env::temp_dir().join(format!("kel-sessions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let read = |name, place| std::fs::read_to_string(file_in(&dir, "myapp", name, place)).ok();
        save_in(
            &dir,
            "myapp",
            &[
                output(Some("server"), "listening\r\n"),
                output(None, "hi\r\n"),
                output(None, ""),
            ],
        );
        assert_eq!(read(Some("server"), 1).as_deref(), Some("listening\r\n"));
        assert_eq!(read(None, 2).as_deref(), Some("hi\r\n"));
        assert_eq!(read(None, 3), None); // it showed nothing
        // The second tab was closed: its file goes, the first one's is new.
        save_in(&dir, "myapp", &[output(Some("server"), "stopped\r\n")]);
        assert_eq!(read(Some("server"), 1).as_deref(), Some("stopped\r\n"));
        assert_eq!(read(None, 2), None);
        // No tab showed anything: no folder either.
        save_in(&dir, "myapp", &[output(None, "")]);
        assert!(!dir.join("myapp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
