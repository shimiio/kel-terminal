//! Shell integration: the shell tells Kel things only it knows.
//!
//! Kel sees bytes, not what they mean: after `cd C:\Windows` the folder changed inside
//! the shell, and nothing in the output says so. So Kel asks the shell to say it: before
//! each prompt, the shell sends `ESC]9;9;<folder> ESC\` (the same code Windows Terminal
//! uses). It's an `ESC]` code like the title, so it doesn't show on the screen.
//!
//! - **PowerShell** (`powershell.exe`, `pwsh.exe`): Kel starts it with `-NoExit -Command`
//!   and a script that wraps the `prompt` function. Your profile runs first, so a
//!   prompt of your own (oh-my-posh, starship) stays, the code goes before it.
//!   A side effect of `-Command`: PowerShell doesn't print its logo at start.
//! - **cmd**: its prompt comes from the `PROMPT` variable, where `$e` is ESC and `$P`
//!   the folder. Kel sets it for every shell, so a `cmd` started from PowerShell
//!   reports its folder too.
//! - Other shells: nothing; Kel just doesn't know their folder.

use portable_pty::CommandBuilder;
use std::path::Path;

/// Wraps `prompt`: its text, with the folder's code before it. Only a real folder:
/// in `HKLM:` (the registry) or `Env:` PowerShell isn't in one.
///
/// No `"` anywhere: Windows hands the script over inside `"..."`, and PowerShell 5
/// mixes up `"` inside it. Hence `[char]27` (ESC) and single quotes.
const POWERSHELL: &str = r"$global:__kel_prompt = $function:prompt
function global:prompt {
  $loc = $executionContext.SessionState.Path.CurrentLocation
  $osc = ''
  if ($loc.Provider.Name -eq 'FileSystem') { $osc = [char]27 + ']9;9;' + $loc.ProviderPath + [char]27 + '\' }
  $osc + (& $global:__kel_prompt)
}";

/// cmd's code for the folder, before its prompt.
const CMD_PROMPT: &str = "$e]9;9;$P$e\\";

/// Adds shell integration to the command that starts `shell`.
pub fn add(cmd: &mut CommandBuilder, shell: &str) {
    let name = Path::new(shell)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase());
    if let Some("powershell" | "pwsh") = name.as_deref() {
        cmd.args(["-NoExit", "-Command", POWERSHELL]);
    }
    cmd.env("PROMPT", cmd_prompt(std::env::var("PROMPT").ok()));
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
    fn powershell_script_has_no_double_quotes() {
        assert!(!POWERSHELL.contains('"'));
    }
}
