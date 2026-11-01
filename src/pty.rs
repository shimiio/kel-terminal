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
    introduce_kel(&mut cmd);
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

/// Environment variables that tell the programs in a tab where they run. Every
/// process gets a copy of its parent's, so they reach nvim too: Kel → shell → nvim.
/// - `TERM_PROGRAM=kel`, `TERM_PROGRAM_VERSION=0.1.0`: which terminal this is
///   (nvim's `:checkhealth` shows it). Replaces the one Kel itself may have got,
///   e.g. `vscode` when started from VS Code's terminal.
/// - `LANG=en_US.UTF-8`, from Windows' display language, unless it's set already.
///   Programs born on Unix (nvim, git, less) read their language from it. On Windows
///   nobody sets it, and nvim's `:checkhealth` reports "Locale does not support UTF-8".
///   Not `TERM`: nvim talks to ConPTY, not to Kel, and knows ConPTY without it.
fn introduce_kel(cmd: &mut CommandBuilder) {
    cmd.env("TERM_PROGRAM", "kel");
    cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    if std::env::var_os("LANG").is_none()
        && let Some(lang) = display_language().as_deref().and_then(lang)
    {
        cmd.env("LANG", lang);
    }
}

/// The `LANG` for a Windows language name: `en-US` → `en_US.UTF-8`,
/// `zh-Hans-CN` → `zh_CN.UTF-8`. `None` without a country (`ru`, `en-001`): `LANG`
/// wants both.
fn lang(windows: &str) -> Option<String> {
    let mut parts = windows.split('-');
    let language = parts.next().filter(|part| {
        (2..=3).contains(&part.len()) && part.chars().all(|c| c.is_ascii_alphabetic())
    })?;
    let country =
        parts.find(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))?;
    Some(format!(
        "{}_{}.UTF-8",
        language.to_ascii_lowercase(),
        country.to_ascii_uppercase()
    ))
}

/// The language Windows shows its menus in, the first one in Settings → Language,
/// e.g. `en-US`. (Not the date and number format: git and nvim speak the menus'
/// language when `LANG` isn't set, so they stay as they were.)
#[cfg(windows)]
fn display_language() -> Option<String> {
    /// "Give names like `en-US`", not numbers.
    const MUI_LANGUAGE_NAME: u32 = 0x8;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserPreferredUILanguages(
            flags: u32,
            count: *mut u32,
            names: *mut u16,
            length: *mut u32,
        ) -> i32;
    }
    // The names come one after another, each ending with a 0: "en-US\0ru-RU\0\0".
    let mut names = [0u16; 256];
    let mut length = names.len() as u32;
    let mut count = 0;
    // Safety: `length` says how long `names` is; Windows writes at most that much.
    let ok = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            names.as_mut_ptr(),
            &mut length,
        )
    };
    let first = names.split(|&c| c == 0).next()?;
    (ok != 0 && !first.is_empty()).then(|| String::from_utf16_lossy(first))
}

/// Linux and macOS: the system or the desktop sets `LANG` (or Kel will, later).
#[cfg(not(windows))]
fn display_language() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_from_windows_language_names() {
        assert_eq!(lang("en-US").as_deref(), Some("en_US.UTF-8"));
        assert_eq!(lang("uk-UA").as_deref(), Some("uk_UA.UTF-8"));
        assert_eq!(lang("zh-Hans-CN").as_deref(), Some("zh_CN.UTF-8"));
        assert_eq!(lang("sr-Latn-RS").as_deref(), Some("sr_RS.UTF-8"));
        assert_eq!(lang("ru"), None);
        assert_eq!(lang("en-001"), None);
        assert_eq!(lang(""), None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_has_a_display_language() {
        let name = display_language().expect("a language");
        assert!(name.contains('-'), "{name}");
    }
}
