//! A project's own history of commands: what ↑ brings back in its tabs.
//!
//! The shell keeps the history, not Kel: PowerShell (its PSReadLine), bash and
//! zsh each write the lines you ran to one file, the same for every window on
//! the computer. So ↑ in a project's tab brought back what was typed anywhere,
//! in home or in another project. Kel gives a project's tab a file of its own:
//!
//! ```text
//! %APPDATA%\kel\history\myapp\tab-server.txt   the tab named "server"
//! %APPDATA%\kel\history\myapp\project.txt      its tabs without a name
//! ```
//!
//! - A tab with a name has its own history, found again by the name the next
//!   time the project opens. Its place in the bar won't do: tabs move.
//! - The tabs without a name share the project's.
//! - Home's tabs keep the shell's usual file, as in any terminal: what you ran
//!   elsewhere is there. A project's tab starts with an empty history.
//! - cmd keeps no history between starts: nothing to keep apart.
//!
//! The file's path goes to the shell as `KEL_HISTORY`, and the shell
//! integration tells the shell to use it (see `shell_integration`). A shell
//! reads it once, at its start: a tab renamed while it runs writes to its old
//! file until it's started again.
//!
//! Nothing here is an error to show: a history that can't be kept apart is
//! the shell's usual one.

use crate::config;
use std::path::{Path, PathBuf};

/// The history file of a tab of `project`: by its name `tab`, or the project's
/// for one without. Its folder is made, so the shell can write there. `None`
/// if it can't be: the shell keeps its usual history.
pub fn file(project: &str, tab: Option<&str>) -> Option<PathBuf> {
    let file = file_in(&dir()?, project, tab);
    std::fs::create_dir_all(file.parent()?).ok()?;
    Some(file)
}

/// The tab `old` of `project` is named `new` now: its history goes with it,
/// unless a tab of that name left one already.
pub fn rename_tab(project: &str, old: &str, new: &str) {
    if let Some(dir) = dir() {
        rename_tab_in(&dir, project, old, new);
    }
}

/// The project `old` is named `new` now: its tabs' histories go with it.
pub fn rename_project(old: &str, new: &str) {
    if let Some(dir) = dir() {
        moved(&dir.join(old), &dir.join(new));
    }
}

/// The project `name` is deleted: the commands typed in it go too.
pub fn delete_project(name: &str) {
    if let Some(dir) = dir() {
        let _ = std::fs::remove_dir_all(dir.join(name));
    }
}

/// Where the histories are: `history` in Kel's folder, a folder per project.
fn dir() -> Option<PathBuf> {
    Some(config::dir().ok()?.join("history"))
}

/// `file`, in `dir`, nothing made.
fn file_in(dir: &Path, project: &str, tab: Option<&str>) -> PathBuf {
    let name = match tab {
        Some(tab) => format!("tab-{}.txt", file_name(tab)),
        None => "project.txt".to_string(),
    };
    dir.join(project).join(name)
}

/// A tab's name as a part of a file's name: what Windows won't have in one
/// (`\ / : * ? " < > |`) is `_`. Never a name by itself (`tab-con.txt`), so a
/// tab named `con` or ending in a dot is fine.
fn file_name(tab: &str) -> String {
    let clean = |c: char| {
        if c.is_control() || "\\/:*?\"<>|".contains(c) {
            '_'
        } else {
            c
        }
    };
    tab.chars().take(64).map(clean).collect()
}

/// `rename_tab`, in `dir`.
fn rename_tab_in(dir: &Path, project: &str, old: &str, new: &str) {
    let (from, to) = (
        file_in(dir, project, Some(old)),
        file_in(dir, project, Some(new)),
    );
    moved(&from, &to);
}

/// `from` is `to` now, if there's a `from` and no `to` yet: a history is never
/// written over.
fn moved(from: &Path, to: &Path) {
    if from.exists() && !to.exists() {
        let _ = std::fs::rename(from, to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_per_named_tab() {
        let dir = Path::new("history");
        let project = dir.join("myapp");
        assert_eq!(
            file_in(dir, "myapp", Some("server")),
            project.join("tab-server.txt")
        );
        // Without a name: the project's, which no tab's name can be.
        assert_eq!(file_in(dir, "myapp", None), project.join("project.txt"));
        assert_eq!(
            file_in(dir, "myapp", Some("project")),
            project.join("tab-project.txt")
        );
        // What a file's name can't have.
        assert_eq!(file_name("a/b: c?"), "a_b_ c_");
        assert_eq!(file_name("con"), "con");
        assert_eq!(file_name(&"x".repeat(100)).len(), 64);
    }

    #[test]
    fn a_renamed_tab_keeps_its_history() {
        let dir = std::env::temp_dir().join(format!("kel-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = |tab| file_in(&dir, "myapp", Some(tab));
        std::fs::create_dir_all(dir.join("myapp")).unwrap();
        std::fs::write(file("server"), "npm run dev\n").unwrap();
        rename_tab_in(&dir, "myapp", "server", "api");
        assert!(!file("server").exists());
        assert_eq!(
            std::fs::read_to_string(file("api")).unwrap(),
            "npm run dev\n"
        );
        // Not over another tab's: both stay as they are.
        std::fs::write(file("web"), "vite\n").unwrap();
        rename_tab_in(&dir, "myapp", "web", "api");
        assert_eq!(
            std::fs::read_to_string(file("api")).unwrap(),
            "npm run dev\n"
        );
        assert!(file("web").exists());
        // No history yet: nothing to move, no error.
        rename_tab_in(&dir, "myapp", "none", "other");
        assert!(!file("other").exists());
        // The project renamed: the folder.
        moved(&dir.join("myapp"), &dir.join("shop"));
        assert!(file_in(&dir, "shop", Some("api")).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
