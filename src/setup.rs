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
    /// Its line in the command palette, and what's at that line's right.
    pub label: &'static str,
    pub hint: &'static str,
    /// The script itself.
    pub text: &'static str,
    /// What it's told, after `-File`: for Kel's own, which kel.exe (see `command`).
    pub args: &'static [&'static str],
}

/// In a script's `args`: the kel.exe that runs now, as `-From "<path>"`.
pub const THIS_EXE: &str = "-From <this kel.exe>";

/// PowerShell scripts: Windows only. Linux and macOS will get `.sh` ones.
#[cfg(windows)]
pub const SCRIPTS: &[Script] = &[
    // Kel itself, from the kel.exe you downloaded: like any program.
    Script {
        name: "kel",
        label: "Install Kel on this computer",
        hint: "Start menu, PATH, Explorer",
        text: include_str!("../setup/kel.ps1"),
        args: &[THIS_EXE],
    },
    Script {
        name: "kel-uninstall",
        label: "Uninstall Kel from this computer",
        hint: "your config stays",
        text: include_str!("../setup/kel.ps1"),
        args: &["-Uninstall"],
    },
    Script {
        name: "lazygit",
        label: "Install lazygit: git in the terminal",
        hint: "setup script",
        text: include_str!("../setup/lazygit.ps1"),
        args: &[],
    },
];

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
            label: script.label.into(),
            hint: script.hint.into(),
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

/// The command that runs `script`, written at `path`, typed into a new tab. It
/// works in PowerShell, pwsh and cmd alike.
/// - `-NoProfile`: without your profile, so it starts fast and the same everywhere.
/// - `-ExecutionPolicy Bypass`: Windows doesn't run `.ps1` files at all by default
///   (the `Restricted` policy). Bypass is for this one run, nothing else changes.
/// - Paths in quotes: a user name can have a space in it.
/// - Its `args`; `THIS_EXE` is `exe`, the kel.exe that runs.
pub fn command(script: &Script, path: &Path, exe: &Path) -> String {
    let mut command = format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\"",
        path.display()
    );
    for arg in script.args {
        match *arg {
            THIS_EXE => command += &format!(" -From \"{}\"", exe.display()),
            arg => command += &format!(" {arg}"),
        }
    }
    command
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
        let script = Script {
            name: "lazygit",
            label: "",
            hint: "",
            text: "",
            args: &[],
        };
        let path = Path::new(r"C:\Users\Jo Ann\AppData\Local\Temp\kel\lazygit.ps1");
        assert_eq!(
            command(&script, path, Path::new("kel.exe")),
            r#"powershell -NoProfile -ExecutionPolicy Bypass -File "C:\Users\Jo Ann\AppData\Local\Temp\kel\lazygit.ps1""#
        );
    }

    #[test]
    fn kel_s_own_is_told_which_exe() {
        let script = Script {
            name: "kel",
            label: "",
            hint: "",
            text: "",
            args: &[THIS_EXE, "-NoMenu"],
        };
        let exe = Path::new(r"C:\Users\Jo Ann\Downloads\kel.exe");
        assert_eq!(
            command(&script, Path::new("kel.ps1"), exe),
            r#"powershell -NoProfile -ExecutionPolicy Bypass -File "kel.ps1" -From "C:\Users\Jo Ann\Downloads\kel.exe" -NoMenu"#
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
