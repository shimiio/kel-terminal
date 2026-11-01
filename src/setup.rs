//! Install scripts: the ones in the repository's `setup/` folder, built into kel.exe.
//!
//! `include_str!` reads a file when Kel is *built* and puts its text into the exe,
//! like the icons. So kel.exe can run them anywhere, without the repository; and
//! after a change to a script, the next build carries the new one.
//!
//! To run one, Kel writes it to a file in the temp folder and opens a new tab that
//! runs it there: you see the command, what winget prints, and can answer questions.

use crate::keys::Action;
use crate::palette::Item;
use std::path::{Path, PathBuf};

pub struct Script {
    /// What it installs, e.g. "lazygit". Also its file's name.
    pub name: &'static str,
    /// Its line in the command palette.
    pub label: &'static str,
    /// The script itself.
    pub text: &'static str,
}

/// PowerShell scripts: Windows only. Linux and macOS will get `.sh` ones.
#[cfg(windows)]
pub const SCRIPTS: &[Script] = &[Script {
    name: "lazygit",
    label: "Install lazygit: git in the terminal",
    text: include_str!("../setup/lazygit.ps1"),
}];

#[cfg(not(windows))]
pub const SCRIPTS: &[Script] = &[];

pub fn find(name: &str) -> Option<&'static Script> {
    SCRIPTS.iter().find(|script| script.name == name)
}

/// The scripts' lines in the command palette.
pub fn palette_items() -> Vec<Item> {
    SCRIPTS
        .iter()
        .map(|script| Item {
            label: script.label,
            hint: "setup script",
            action: Action::Install(script.name),
        })
        .collect()
}

/// Writes `script` to `%TEMP%\kel\<name>.ps1` (again each time: always this
/// build's version), and returns where.
pub fn write_to_temp(script: &Script) -> std::io::Result<PathBuf> {
    let folder = std::env::temp_dir().join("kel");
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(format!("{}.ps1", script.name));
    std::fs::write(&path, script.text)?;
    Ok(path)
}

/// The command that runs the script at `path`, typed into a new tab. It works in
/// PowerShell, pwsh and cmd alike.
/// - `-NoProfile`: without your profile, so it starts fast and the same everywhere.
/// - `-ExecutionPolicy Bypass`: Windows doesn't run `.ps1` files at all by default
///   (the `Restricted` policy). Bypass is for this one run, nothing else changes.
/// - The path in quotes: a user name can have a space in it.
pub fn command(path: &Path) -> String {
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\"",
        path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_are_plain_ascii() {
        // PowerShell 5 reads a .ps1 without a BOM in the old Windows encoding:
        // anything past ASCII (a ✓, a «) would come out garbled.
        for script in SCRIPTS {
            assert!(!script.text.is_empty(), "{}", script.name);
            assert!(script.text.is_ascii(), "{} has non-ASCII text", script.name);
        }
    }

    #[test]
    fn every_script_is_in_the_palette() {
        let items = palette_items();
        for script in SCRIPTS {
            assert!(
                items
                    .iter()
                    .any(|item| item.action == Action::Install(script.name))
            );
            assert!(find(script.name).is_some());
        }
    }

    #[test]
    fn the_command_quotes_the_path() {
        let path = Path::new(r"C:\Users\Jo Ann\AppData\Local\Temp\kel\lazygit.ps1");
        assert_eq!(
            command(path),
            r#"powershell -NoProfile -ExecutionPolicy Bypass -File "C:\Users\Jo Ann\AppData\Local\Temp\kel\lazygit.ps1""#
        );
    }

    #[cfg(windows)]
    #[test]
    fn written_to_temp_as_it_is() {
        let script = find("lazygit").unwrap();
        let path = write_to_temp(script).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), script.text);
    }
}
