//! Projects: `%APPDATA%\kel\projects\<name>.toml`, a recipe for a set of tabs.
//!
//! `kel <name>` opens one: each tab starts in its folder and types its command, as
//! if you did. The file holds no running programs (those can't be saved): Kel
//! starts them again.
//!
//! When the window closes, Kel writes the file again with the tabs open then
//! (see `save`): the project is what you left open.
//!
//! ```toml
//! folder = 'C:\code\kel-terminal'   # single quotes: a '\' stays a '\'
//! color = "#e0af68"                 # optional: its name and icons in this color
//!
//! [[tab]]
//! name = "editor"
//! command = "nvim ."
//!
//! [[tab]]
//! folder = "docs"   # inside the project's folder
//! ```

use crate::config;
use crate::keys::Action;
use crate::palette::Item;
use crate::sidebar::Entry;
use crate::theme;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// On top of a file Kel wrote.
const HEADER: &str = "\
# Kel writes this file when you leave the project or close the window: its tabs
# as they were then.
# Edit it while the project isn't open, or Kel writes over it. Comments don't stay.

";

/// `deny_unknown_fields`: a typo (`[[tabs]]`, `comand`) is an error, not a tab
/// that silently goes missing.
/// `skip_serializing_if`: a line that says nothing isn't written.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// The project's folder: tabs start here; their own folders are inside it.
    pub folder: PathBuf,
    /// The project's own accent color: its name before the tabs and the tab
    /// icons. Without it, none: gray (or the theme's `accent`, if it has one).
    /// A window then says which project
    /// it is at a glance.
    #[serde(
        default,
        deserialize_with = "theme::optional_hex",
        serialize_with = "theme::write_hex",
        skip_serializing_if = "Option::is_none"
    )]
    pub color: Option<u32>,
    /// `[[tab]]` in the file, one per tab, left to right.
    #[serde(default, rename = "tab", skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<TabRecipe>,
}

/// One tab of a project. Every line is optional: `[[tab]]` alone is a shell in
/// the project's folder.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TabRecipe {
    /// Shown in the tab bar instead of the program's title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where it starts: inside the project's folder (`"src"`), or a full path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// Typed into the shell once it's ready, e.g. `nvim .`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

impl Project {
    /// The tabs to open: the file's, or one plain tab if it lists none.
    pub fn tabs(&self) -> Vec<TabRecipe> {
        if self.tabs.is_empty() {
            vec![TabRecipe::default()]
        } else {
            self.tabs.clone()
        }
    }

    /// Where `tab` starts. A full path in `tab.folder` replaces the project's
    /// (that's what `join` does with one).
    pub fn folder_of(&self, tab: &TabRecipe) -> PathBuf {
        match &tab.folder {
            Some(folder) => self.folder.join(folder),
            None => self.folder.clone(),
        }
    }

    /// The other way round: `folder` as a tab's `folder` line. Inside the project's
    /// folder: the rest of the path (`src\term`); the folder itself: no line;
    /// elsewhere: the full path. Big and small letters are the same folder on
    /// Windows, and the shell reports them as typed (`cd SRC`).
    pub fn tab_folder(&self, folder: &Path) -> Option<PathBuf> {
        let mut rest = folder.components();
        let inside = self.folder.components().all(|part| {
            rest.next()
                .is_some_and(|next| next.as_os_str().eq_ignore_ascii_case(part.as_os_str()))
        });
        match inside {
            true if rest.as_path().as_os_str().is_empty() => None,
            true => Some(rest.as_path().to_path_buf()),
            false => Some(folder.to_path_buf()),
        }
    }

    /// The file's text, as Kel writes it.
    fn to_file(&self) -> anyhow::Result<String> {
        Ok(format!("{HEADER}{}", toml::to_string(self)?))
    }
}

/// Where the projects live, e.g. `C:\Users\you\AppData\Roaming\kel\projects`.
pub fn dir() -> anyhow::Result<PathBuf> {
    Ok(config::dir()?.join("projects"))
}

/// Reads the project `name` (its file's name, without `.toml`).
pub fn load(name: &str) -> anyhow::Result<Project> {
    let file = format!("{name}.toml");
    let path = dir()?.join(&file);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("no project \"{name}\": can't read {}", path.display()))?;
    config::parse_toml(&file, &text)
}

/// The projects in `folder`: its `.toml` files' names, without `.toml`, A to Z.
pub fn names_in(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new(); // no projects yet: no folder either
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .filter_map(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

/// The projects as command palette lines: "Open project: kel", with its folder
/// at the right. `current` is the project on screen: "here".
pub fn palette_items(current: Option<&str>) -> Vec<Item> {
    let Ok(dir) = dir() else {
        return Vec::new();
    };
    names_ordered(&dir)
        .into_iter()
        .map(|name| {
            let hint = if current == Some(name.as_str()) {
                "here".to_string()
            } else {
                match load(&name) {
                    Ok(project) => short_folder(&project.folder),
                    // Opening it shows why, in the tab bar.
                    Err(_) => "can't read its file".to_string(),
                }
            };
            Item {
                label: format!("Open project: {name}"),
                hint,
                action: Action::OpenProject(name),
            }
        })
        .collect()
}

/// The projects as the sidebar lists them: each with its color and folder.
/// `current` is the project on screen: "here".
pub fn sidebar_entries(current: Option<&str>) -> Vec<Entry> {
    let Ok(dir) = dir() else {
        return Vec::new();
    };
    let entries = names_ordered(&dir).into_iter().map(|name| {
        let here = current == Some(name.as_str());
        entry(name, here)
    });
    entries.collect()
}

/// The projects in `folder`, in the order you put them in the sidebar (see
/// `ordered`).
fn names_ordered(folder: &Path) -> Vec<String> {
    ordered(names_in(folder), &read_order(folder))
}

/// `names` (the projects there are, by name) in the order `order` lists them:
/// the ones it lists first, as listed; the rest after them, by name (new ones).
/// A name it lists that's gone is left out.
fn ordered(mut names: Vec<String>, order: &[String]) -> Vec<String> {
    let mut sorted: Vec<String> = Vec::new();
    for name in order {
        if names.contains(name) && !sorted.contains(name) {
            sorted.push(name.clone());
        }
    }
    names.retain(|name| !sorted.contains(name));
    sorted.extend(names);
    sorted
}

/// The order of the projects: `order.txt` next to them, a name a line. None
/// written yet: an empty one (by name, then).
fn read_order(folder: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(folder.join(ORDER_FILE)).unwrap_or_default();
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

/// Keeps `names` as the projects' order (dragged in the sidebar, or Alt+↑↓).
pub fn save_order(names: &[String]) -> anyhow::Result<()> {
    write_order(&dir()?, names)
}

/// Writes `names` as the order of the projects in `folder`.
fn write_order(folder: &Path, names: &[String]) -> anyhow::Result<()> {
    std::fs::create_dir_all(folder)?;
    let text: String = names.iter().map(|name| format!("{name}\n")).collect();
    std::fs::write(folder.join(ORDER_FILE), text).context("can't save the projects' order")
}

/// Deletes the project `name`: its file, and its line in the order. Its folder,
/// the code in it, isn't touched: Kel never deletes your files. Gone from the
/// recent ones too, which are its file's time.
pub fn delete(name: &str) -> anyhow::Result<()> {
    delete_in(&dir()?, name)
}

/// `delete`, of a project in `folder`.
fn delete_in(folder: &Path, name: &str) -> anyhow::Result<()> {
    std::fs::remove_file(folder.join(format!("{name}.toml")))
        .with_context(|| format!("can't delete project \"{name}\""))?;
    let order = read_order(folder);
    if order.iter().any(|n| n == name) {
        let order: Vec<String> = order.into_iter().filter(|n| n != name).collect();
        write_order(folder, &order)?;
    }
    Ok(())
}

/// The projects' order, next to their files. Not `.toml`: not a project.
const ORDER_FILE: &str = "order.txt";

/// The projects used last, newest first, at most `count`, for the start screen:
/// by when their file was written, which Kel does when you leave a project or
/// close the window.
pub fn recent_entries(count: usize) -> Vec<Entry> {
    let Ok(dir) = dir() else {
        return Vec::new();
    };
    let written = names_in(&dir).into_iter().map(|name| {
        let file = dir.join(format!("{name}.toml"));
        let time = std::fs::metadata(file).and_then(|m| m.modified());
        (time.unwrap_or(SystemTime::UNIX_EPOCH), name)
    });
    let names = newest_first(written.collect(), count);
    names.into_iter().map(|name| entry(name, false)).collect()
}

/// The names of the `count` newest, newest first. The same time (two files
/// written in one go): by name.
fn newest_first(mut written: Vec<(SystemTime, String)>, count: usize) -> Vec<String> {
    written.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    written
        .into_iter()
        .take(count)
        .map(|(_, name)| name)
        .collect()
}

/// The project `name` as a row of the sidebar or the start screen: its color,
/// and where it is ("here" if it's on screen).
fn entry(name: String, here: bool) -> Entry {
    let (color, hint) = match load(&name) {
        Ok(project) if here => (project.color, "here".to_string()),
        Ok(project) => (project.color, short_folder(&project.folder)),
        Err(_) => (None, "can't read its file".to_string()),
    };
    Entry {
        name,
        color,
        hint,
        current: here,
        live: Default::default(), // App knows: `update_live`
    }
}

/// The palette's line for saving the tabs as the project `typed`: "Save the tabs
/// as project: myapp", with `folder` (where it will be) at the right. None for a
/// name that can't be a file's, or a project that's already there: its "Open
/// project" line is listed instead.
pub fn save_item(typed: &str, folder: &Path, existing: &[String]) -> Option<Item> {
    let name = typed.trim();
    let taken = existing.iter().any(|e| e.eq_ignore_ascii_case(name));
    if !valid_name(name) || taken {
        return None;
    }
    Some(Item {
        label: format!("Save the tabs as project: {name}"),
        hint: short_folder(folder),
        action: Action::SaveProject(name.to_string()),
    })
}

/// Whether `name` can be a project's file name on Windows: not empty, none of
/// `\ / : * ? " < > |`, no `.` or space at the end (Windows drops them), not a
/// device name like `CON` or `COM1`, and not too long to read in the tab bar.
pub fn valid_name(name: &str) -> bool {
    const DEVICES: [&str; 4] = ["con", "prn", "aux", "nul"];
    let lower = name.to_ascii_lowercase();
    let device = DEVICES.contains(&lower.as_str())
        || ["com", "lpt"].iter().any(|d| {
            lower
                .strip_prefix(d)
                .is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit())
        });
    let bad_char = |c: char| c.is_control() || "\\/:*?\"<>|".contains(c);
    !name.is_empty()
        && name.chars().count() <= 64
        && !name.contains(bad_char)
        && !name.ends_with(['.', ' '])
        && !device
}

/// Renames the project `old` to `new`: its file. Not to a name a file can't
/// have, and not over another project.
pub fn rename(old: &str, new: &str) -> anyhow::Result<()> {
    if !valid_name(new) {
        anyhow::bail!("\"{new}\" can't be a project's name");
    }
    // "kel" → "Kel" is the same file on Windows: not another project.
    if !new.eq_ignore_ascii_case(old) && exists(new) {
        anyhow::bail!("project \"{new}\" already exists");
    }
    let dir = dir()?;
    let file = |name: &str| dir.join(format!("{name}.toml"));
    std::fs::rename(file(old), file(new))
        .with_context(|| format!("can't rename project \"{old}\""))?;
    // Its place in the order stays.
    let order = read_order(&dir);
    if order.iter().any(|name| name == old) {
        let order: Vec<String> = order
            .into_iter()
            .map(|name| if name == old { new.to_string() } else { name })
            .collect();
        save_order(&order)?;
    }
    Ok(())
}

/// Writes `color` into the file of the project `name`, which isn't open (an open
/// one keeps its color in the window, and writes it with its tabs).
pub fn set_color(name: &str, color: Option<u32>) -> anyhow::Result<()> {
    let mut project = load(name)?;
    project.color = color;
    save(name, &project)
}

/// Whether the project `name` has a file already.
pub fn exists(name: &str) -> bool {
    dir().is_ok_and(|dir| dir.join(format!("{name}.toml")).exists())
}

/// A folder's last two parts, enough to tell projects apart:
/// `C:\code\major-projects\kel-terminal` → `…\major-projects\kel-terminal`.
fn short_folder(folder: &Path) -> String {
    let parts: Vec<_> = folder.components().collect();
    // `C:\code` is 3 parts: `C:`, `\`, `code`. Short already.
    if parts.len() <= 4 {
        return folder.display().to_string();
    }
    let tail: PathBuf = parts[parts.len() - 2..].iter().collect();
    format!("…{}{}", std::path::MAIN_SEPARATOR, tail.display())
}

/// Writes the project `name`'s file, over the old one.
pub fn save(name: &str, project: &Project) -> anyhow::Result<()> {
    let path = dir()?.join(format!("{name}.toml"));
    let save = || -> anyhow::Result<()> {
        // First to a file next to it, then in its place: if Kel stops halfway
        // (the PC turns off), the old file is still whole.
        let new = path.with_extension("toml.new");
        // The first project ever: no folder for them yet.
        std::fs::create_dir_all(dir()?)?;
        std::fs::write(&new, project.to_file()?)?;
        std::fs::rename(&new, &path)?;
        Ok(())
    };
    save().with_context(|| format!("can't save project \"{name}\" to {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_order_of_the_projects() {
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let there = names(&["api", "kel", "notes", "web"]);
        // No order yet: by name.
        assert_eq!(ordered(there.clone(), &[]), there);
        // web and kel moved up; notes and api, not listed, after them by name;
        // "old" is gone, a second "kel" doesn't count.
        let order = names(&["web", "old", "kel", "kel"]);
        assert_eq!(ordered(there, &order), ["web", "kel", "api", "notes"]);
    }

    #[test]
    fn deleting_a_project() {
        let folder = std::env::temp_dir().join(format!("kel-test-delete-{}", std::process::id()));
        let code = folder.join("code");
        std::fs::create_dir_all(&code).unwrap();
        for name in ["api", "web"] {
            std::fs::write(folder.join(format!("{name}.toml")), "folder = 'x'\n").unwrap();
        }
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        write_order(&folder, &names(&["web", "api"])).unwrap();
        delete_in(&folder, "web").unwrap();
        // Its file and its line go; the others and any other folder stay.
        assert_eq!(names_in(&folder), ["api"]);
        assert_eq!(read_order(&folder), ["api"]);
        assert!(code.is_dir());
        // Not there: an error, nothing else changes.
        assert!(delete_in(&folder, "web").is_err());
        assert_eq!(names_in(&folder), ["api"]);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn recent_projects_newest_first() {
        let at = |s| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(s);
        let written = vec![
            (at(10), "old".to_string()),
            (at(30), "web".to_string()),
            (at(20), "kel".to_string()),
            (at(30), "api".to_string()), // the same time as web: by name
        ];
        assert_eq!(newest_first(written.clone(), 3), ["api", "web", "kel"]);
        assert_eq!(newest_first(written, 10).len(), 4);
    }

    const EXAMPLE: &str = r#"
folder = 'C:\code\kel'

[[tab]]
name = "editor"
command = "nvim ."

[[tab]]
folder = "docs"

[[tab]]
folder = 'D:\other'
"#;

    #[test]
    fn reads_a_project() {
        let project: Project = config::parse_toml("kel.toml", EXAMPLE).unwrap();
        assert_eq!(project.folder, PathBuf::from(r"C:\code\kel"));
        assert_eq!(project.tabs.len(), 3);
        assert_eq!(project.tabs[0].name.as_deref(), Some("editor"));
        assert_eq!(project.tabs[0].command.as_deref(), Some("nvim ."));
        assert_eq!(project.tabs[1].command, None);
    }

    #[test]
    fn tab_folders() {
        let project: Project = config::parse_toml("kel.toml", EXAMPLE).unwrap();
        let folders: Vec<PathBuf> = project
            .tabs()
            .iter()
            .map(|t| project.folder_of(t))
            .collect();
        assert_eq!(
            folders,
            [r"C:\code\kel", r"C:\code\kel\docs", r"D:\other"].map(PathBuf::from)
        );
    }

    #[test]
    fn no_tabs_is_one_plain_tab() {
        let project: Project = config::parse_toml("kel.toml", "folder = 'C:\\code'").unwrap();
        assert_eq!(project.tabs(), [TabRecipe::default()]);
    }

    #[test]
    fn folders_as_tab_folders() {
        let project: Project = config::parse_toml("kel.toml", EXAMPLE).unwrap();
        let tab_folder = |folder: &str| project.tab_folder(Path::new(folder));
        assert_eq!(tab_folder(r"C:\code\kel"), None);
        assert_eq!(tab_folder(r"c:\Code\KEL\"), None);
        assert_eq!(
            tab_folder(r"C:\code\kel\src\term"),
            Some(r"src\term".into())
        );
        assert_eq!(tab_folder(r"C:\code\kelp"), Some(r"C:\code\kelp".into()));
        assert_eq!(tab_folder(r"C:\code"), Some(r"C:\code".into()));
    }

    #[test]
    fn the_color_reads_and_stays() {
        let text = "folder = 'C:\\code'\ncolor = \"#E0af68\"\n";
        let project: Project = config::parse_toml("kel.toml", text).unwrap();
        assert_eq!(project.color, Some(0xe0af68));
        // Written back in small letters, right after the folder.
        let saved = project.to_file().unwrap();
        assert!(saved.contains("color = \"#e0af68\""), "{saved}");
        // No color: no line.
        let plain: Project = config::parse_toml("kel.toml", "folder = 'C:\\code'").unwrap();
        assert_eq!(plain.color, None);
        assert!(!plain.to_file().unwrap().contains("color"));
        let bad = config::parse_toml::<Project>("kel.toml", "folder = 'C:'\ncolor = \"orange\"");
        assert!(bad.unwrap_err().to_string().contains("not a color"));
    }

    #[test]
    fn saved_file_reads_back() {
        let project: Project = config::parse_toml("kel.toml", EXAMPLE).unwrap();
        let text = project.to_file().unwrap();
        assert!(text.starts_with(HEADER));
        assert!(text.contains("folder = 'C:\\code\\kel'"), "{text}");
        assert_eq!(
            config::parse_toml::<Project>("kel.toml", &text).unwrap(),
            project
        );
    }

    #[test]
    fn project_names_from_the_files() {
        let folder = std::env::temp_dir().join(format!("kel-test-projects-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for file in ["web.toml", "kel.toml", "notes.txt", "kel.toml.new"] {
            std::fs::write(folder.join(file), "").unwrap();
        }
        assert_eq!(names_in(&folder), ["kel", "web"]);
        std::fs::remove_dir_all(&folder).unwrap();
        // No folder: no projects, no error.
        assert!(names_in(&folder).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn short_folders() {
        let short = |folder: &str| short_folder(Path::new(folder));
        assert_eq!(short(r"C:\code\kel"), r"C:\code\kel");
        assert_eq!(
            short(r"C:\Users\Public\major-projects\kel-terminal"),
            r"…\major-projects\kel-terminal"
        );
    }

    #[test]
    fn project_names() {
        for good in ["myapp", "kel-terminal", "web 2", "notes.v2", "Конспекты"] {
            assert!(valid_name(good), "{good}");
        }
        for bad in [
            "", "a/b", "a\\b", "c:", "what?", "\"q\"", "dot.", "space ", "CON", "com1",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
        assert!(valid_name("console") && valid_name("com10")); // not devices
    }

    #[test]
    fn the_save_line() {
        let existing = ["kel".to_string()];
        let folder = Path::new(r"C:\code\myapp");
        let item = save_item("  myapp ", folder, &existing).unwrap();
        assert_eq!(item.label, "Save the tabs as project: myapp");
        assert_eq!(item.hint, r"C:\code\myapp");
        assert_eq!(item.action, Action::SaveProject("myapp".into()));
        // Taken (in any case), or not a file name: no line.
        assert_eq!(save_item("KEL", folder, &existing), None);
        assert_eq!(save_item("a/b", folder, &existing), None);
    }

    #[test]
    fn typos_are_errors() {
        let typo = "folder = 'C:\\code'\n[[tabs]]\ncommand = 'nvim'";
        let error = config::parse_toml::<Project>("kel.toml", typo).unwrap_err();
        assert!(
            error.to_string().starts_with("kel.toml, line 2:"),
            "{error}"
        );
    }
}
