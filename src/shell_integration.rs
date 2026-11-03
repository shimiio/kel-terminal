//! Shell integration: the shell tells Kel things only it knows.
//!
//! Kel sees bytes, not what they mean: after `cd C:\Windows` the folder changed inside
//! the shell, and nothing in the output says so. So Kel asks the shell to say it, with
//! `ESC]` codes like the title's, which don't show on the screen:
//!
//! | Code | When | Kel then |
//! |---|---|---|
//! | `ESC]9;9;<folder> ESC\` | before each prompt | opens new tabs there |
//! | `ESC]633;E;<command> ESC\` | Enter: the command's text | picks the tab's icon by it |
//! | `ESC]133;C ESC\` | Enter, a command starts | knows the tab is busy |
//! | `ESC]133;D;<0 or 1> ESC\` | before the next prompt: it ended, 0 if it worked | not busy |
//!
//! The same codes as Windows Terminal and VS Code (`633;E` is VS Code's). In the
//! command's text, `\`, `;` and anything below a space are written `\xNN`
//! (`;` is `\x3b`): a real ESC or BEL would end the code early.
//!
//! - **PowerShell** (`powershell.exe`, `pwsh.exe`): Kel starts it with `-NoExit -Command`
//!   and a script that wraps two functions: `prompt` (it makes the prompt's text) and
//!   `PSConsoleHostReadLine` (it reads what you type, until Enter). Your profile runs
//!   first, so a prompt of your own (oh-my-posh, starship) stays, the codes go before it.
//!   A side effect of `-Command`: PowerShell doesn't print its logo at start.
//! - **cmd**: its prompt comes from the `PROMPT` variable, where `$e` is ESC and `$P`
//!   the folder. Kel sets it for every shell, so a `cmd` started from PowerShell
//!   reports its folder too. cmd can't say when a command starts: never busy.
//! - Other shells: nothing; Kel just doesn't know their folder.

use portable_pty::CommandBuilder;
use std::path::Path;

/// The new `prompt`: "the command ended" (from `$?`, did the last one work), then
/// the folder, then the old prompt's text. Only a real folder: in `HKLM:` (the
/// registry) or `Env:` PowerShell isn't in one. Every line resets `$?`, so it's read
/// first, and set back to "failed" for the old prompt, which may show it.
///
/// The new `PSConsoleHostReadLine`: the old one reads the line; once it's done
/// (Enter, and the input is whole: no `{` left open), the command's text, then
/// "the command starts". Not for an empty line: nothing starts.
///
/// No `"` anywhere: Windows hands the script over inside `"..."`, and PowerShell 5
/// mixes up `"` inside it. Hence `[char]27` (ESC) and single quotes.
const POWERSHELL: &str = r"$global:__kel_prompt = $function:prompt
function global:prompt {
  $ok = $global:?
  $e = [string][char]27
  $out = $e + ']133;D;' + [int](-not $ok) + $e + '\'
  $loc = $executionContext.SessionState.Path.CurrentLocation
  if ($loc.Provider.Name -eq 'FileSystem') { $out += $e + ']9;9;' + $loc.ProviderPath + $e + '\' }
  if (-not $ok) { Write-Error failure -ErrorAction Ignore }
  $out + (& $global:__kel_prompt)
}
if (Test-Path Function:\PSConsoleHostReadLine) {
  $global:__kel_readline = $function:PSConsoleHostReadLine
  function global:PSConsoleHostReadLine {
    $line = & $global:__kel_readline
    if ([string]$line -match '\S') {
      $e = [string][char]27
      $text = [regex]::Replace([string]$line, '[\\;\x00-\x1f]', { '\x{0:x2}' -f [int][char]$args[0].Value })
      [Console]::Write($e + ']633;E;' + $text + $e + '\' + $e + ']133;C' + $e + '\')
    }
    $line
  }
}";

/// cmd's code for the folder, before its prompt.
const CMD_PROMPT: &str = "$e]9;9;$P$e\\";

/// Adds shell integration to the command that starts `shell`.
pub fn add(cmd: &mut CommandBuilder, shell: &str) {
    if let Some("powershell" | "pwsh") = name(shell).as_deref() {
        cmd.args(["-NoExit", "-Command", POWERSHELL]);
    }
    cmd.env("PROMPT", cmd_prompt(std::env::var("PROMPT").ok()));
}

/// Whether `shell` reports its folder at each prompt, so Kel can tell when it's
/// ready for typing (see `Tab::type_at_start`).
pub fn reports_folder(shell: &str) -> bool {
    matches!(name(shell).as_deref(), Some("powershell" | "pwsh" | "cmd"))
}

/// The shell's name in lowercase: `C:\...\PowerShell.exe` → `powershell`,
/// `pwsh.exe -NoLogo` → `pwsh`.
fn name(shell: &str) -> Option<String> {
    Path::new(&crate::shell::split(shell).0)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
}

/// cmd's prompt: the folder's code, then the prompt it had (`$P$G`, "C:\Windows>",
/// if none). Kel started from Kel already has the code: not twice.
fn cmd_prompt(current: Option<String>) -> String {
    let current = current.unwrap_or_else(|| "$P$G".to_string());
    if current.contains(CMD_PROMPT) {
        current
    } else {
        format!("{CMD_PROMPT}{current}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_prompt_gets_the_code_once() {
        assert_eq!(cmd_prompt(None), "$e]9;9;$P$e\\$P$G");
        assert_eq!(cmd_prompt(Some("$P$_$G".into())), "$e]9;9;$P$e\\$P$_$G");
        let already = cmd_prompt(None);
        assert_eq!(cmd_prompt(Some(already.clone())), already);
    }

    #[test]
    fn shells_that_report_their_folder() {
        assert!(reports_folder("powershell.exe"));
        assert!(reports_folder("pwsh.exe -NoLogo"));
        assert!(!reports_folder("wsl.exe -d Ubuntu"));
        assert!(reports_folder(r"C:\Program Files\PowerShell\7\pwsh.exe"));
        assert!(reports_folder("CMD.EXE"));
        assert!(!reports_folder("nu.exe"));
    }

    #[test]
    fn powershell_script_has_no_double_quotes() {
        assert!(!POWERSHELL.contains('"'));
    }
}
