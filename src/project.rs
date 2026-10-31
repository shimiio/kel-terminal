//! Projects: `%APPDATA%\kel\projects\<name>.toml`, a recipe for a set of tabs.
//!
//! `kel <name>` opens one: each tab starts in its folder and types its command, as
//! if you did. The file holds no running programs (those can't be saved): Kel
//! starts them again.
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
use anyhow::Context;
use serde::Deserialize;
use std::path::PathBuf;

/// `deny_unknown_fields`: a typo (`[[tabs]]`, `comand`) is an error, not a tab
/// that silently goes missing.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// The project's folder: tabs start here; their own folders are inside it.
    pub folder: PathBuf,
    /// `[[tab]]` in the file, one per tab, left to right.
    #[serde(default, rename = "tab")]
    pub tabs: Vec<TabRecipe>,
}

/// One tab of a project. Every line is optional: `[[tab]]` alone is a shell in
/// the project's folder.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TabRecipe {
    /// Shown in the tab bar instead of the program's title.
    pub name: Option<String>,
    /// Where it starts: inside the project's folder (`"src"`), or a full path.
    pub folder: Option<PathBuf>,
    /// Typed into the shell once it's ready, e.g. `nvim .`
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
    fn typos_are_errors() {
        let typo = "folder = 'C:\\code'\n[[tabs]]\ncommand = 'nvim'";
        let error = config::parse_toml::<Project>("kel.toml", typo).unwrap_err();
        assert!(
            error.to_string().starts_with("kel.toml, line 2:"),
            "{error}"
        );
    }
}
