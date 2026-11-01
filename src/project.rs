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
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// On top of a file Kel wrote.
const HEADER: &str = "\
# Kel writes this file when its window closes: the tabs as they were then.
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
/// at the right. `current` is the project open in this window: "this window".
pub fn palette_items(current: Option<&str>) -> Vec<Item> {
    let Ok(dir) = dir() else {
        return Vec::new();
    };
    names_in(&dir)
        .into_iter()
        .map(|name| {
            let hint = if current == Some(name.as_str()) {
                "this window".to_string()
            } else {
                match load(&name) {
                    Ok(project) => short_folder(&project.folder),
                    // Opening it shows why, in the new window.
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
        std::fs::write(&new, project.to_file()?)?;
        std::fs::rename(&new, &path)?;
        Ok(())
    };
    save().with_context(|| format!("can't save project \"{name}\" to {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn typos_are_errors() {
        let typo = "folder = 'C:\\code'\n[[tabs]]\ncommand = 'nvim'";
        let error = config::parse_toml::<Project>("kel.toml", typo).unwrap_err();
        assert!(
            error.to_string().starts_with("kel.toml, line 2:"),
            "{error}"
        );
    }
}
