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
//! - **bash** (Linux, macOS): Kel writes `BASH` to a file in its folder and starts
//!   `bash --rcfile <it>`. bash reads that file instead of your `~/.bashrc`, so it
//!   reads `~/.bashrc` first, then adds two hooks (see `BASH`). Not Git Bash on
//!   Windows: its folders are `/c/Users/...`, which Windows' Kel can't open.
//! - Other shells: nothing; Kel just doesn't know their folder.

use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

/// bash's hooks, the file bash starts with:
///
/// - `__kel_prompt`, first in `PROMPT_COMMAND`, which bash runs before each
///   prompt: "the command ended" (from `$?`, did the last one work), and the
///   folder. First, so `$?` is still the command's; and it gives `$?` back, for
///   a prompt that shows it.
/// - `__kel_preexec`, the `DEBUG` trap, which bash runs before every command:
///   the first one after a prompt is the line you typed (`$BASH_COMMAND`, its
///   first command: `cargo run` of `cargo run | tee log`), so its text, then
///   "the command starts". An empty line runs nothing but `PROMPT_COMMAND`,
///   whose first command is `__kel_prompt`: then nothing starts. Not if you
///   have a `DEBUG` trap of your own (bash-preexec, atuin): yours stays, and
///   only the ends and the folder come.
///
/// Not `PS0` for the text: it comes before bash adds the line to its history,
/// so `history 1` there is the line before. The `'\'` and `"$bs"` in quotes:
/// unquoted, bash takes a `\` in them as an escape.
const BASH: &str = r#"# Kel's shell integration for bash. Kel writes this file at each start: a change
# here doesn't stay. bash reads it instead of ~/.bashrc, so ~/.bashrc first.
if [ -f ~/.bashrc ]; then . ~/.bashrc; fi

__kel_prompt() {
  local status=$?
  printf '\e]133;D;%s\e\\\e]9;9;%s\e\\' "$(( status != 0 ))" "$PWD"
  __kel_ready=1
  return $status
}

__kel_preexec() {
  [ -n "${__kel_ready-}" ] && [ "$BASH_COMMAND" != __kel_prompt ] || return 0
  __kel_ready=
  local text=$BASH_COMMAND bs='\x5c' sc='\x3b' nl='\x0a' tab='\x09' esc='\x1b' bel='\x07'
  text=${text//'\'/"$bs"}
  text=${text//;/"$sc"}
  text=${text//$'\n'/"$nl"}
  text=${text//$'\t'/"$tab"}
  text=${text//$'\e'/"$esc"}
  text=${text//$'\a'/"$bel"}
  printf '\e]633;E;%s\e\\\e]133;C\e\\' "$text"
}

PROMPT_COMMAND="__kel_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
[ -n "$(trap -p DEBUG)" ] || trap '__kel_preexec' DEBUG
"#;

/// cmd's code for the folder, before its prompt.
const CMD_PROMPT: &str = "$e]9;9;$P$e\\";

/// Adds shell integration to the command that starts `shell`, and the shell's
/// own arguments from the config, `args`, where they go with it.
pub fn add(cmd: &mut CommandBuilder, shell: &str, args: &[String]) {
    match name(shell).as_deref() {
        Some("powershell" | "pwsh") => {
            cmd.args(args);
            // After yours: everything after `-Command` is the command.
            cmd.args(["-NoExit", "-Command", POWERSHELL]);
        }
        // Before yours: bash wants its long options (`--rcfile`) before the
        // short ones (`-l`). Without the file (it can't be written): plain bash.
        Some("bash") if cfg!(unix) => {
            if let Some(file) = bash_file() {
                cmd.arg("--rcfile");
                cmd.arg(file);
            }
            cmd.args(args);
        }
        _ => cmd.args(args),
    }
    cmd.env("PROMPT", cmd_prompt(std::env::var("PROMPT").ok()));
}

/// The file with `BASH`, in Kel's folder: `~/.config/kel/shell/kel.bash`.
/// Written once a Kel runs, if it's not as `BASH` says (a newer Kel's).
fn bash_file() -> Option<&'static Path> {
    static FILE: OnceLock<Option<PathBuf>> = OnceLock::new();
    FILE.get_or_init(|| {
        let file = crate::config::dir().ok()?.join("shell").join("kel.bash");
        if std::fs::read_to_string(&file).ok().as_deref() != Some(BASH) {
            std::fs::create_dir_all(file.parent()?).ok()?;
            std::fs::write(&file, BASH).ok()?;
        }
        Some(file)
    })
    .as_deref()
}

/// Whether `shell` reports its folder at each prompt, so Kel can tell when it's
/// ready for typing (see `Tab::type_at_start`).
pub fn reports_folder(shell: &str) -> bool {
    let name = name(shell);
    let bash = cfg!(unix) && name.as_deref() == Some("bash");
    bash || matches!(name.as_deref(), Some("powershell" | "pwsh" | "cmd"))
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
        assert!(reports_folder("/usr/bin/pwsh"));
        #[cfg(windows)]
        assert!(reports_folder(r"C:\Program Files\PowerShell\7\pwsh.exe"));
        assert!(reports_folder("CMD.EXE"));
        assert!(!reports_folder("nu.exe"));
    }

    /// The hooks in a real bash, in a pseudo-terminal as in a tab: `cd /`,
    /// `false`, a `;` to escape, an empty line, and out. Its own `HOME`, so not
    /// the machine's `~/.bashrc`.
    #[cfg(unix)]
    #[test]
    fn bash_reports_folders_and_commands() {
        use portable_pty::{PtySize, native_pty_system};
        use std::io::{Read, Write};
        use std::time::Duration;

        let home = std::env::temp_dir().join(format!("kel-bash-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let file = home.join("kel.bash");
        std::fs::write(&file, BASH).unwrap();
        let size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = native_pty_system().openpty(size).unwrap();
        let mut cmd = CommandBuilder::new("bash");
        cmd.arg("--rcfile");
        cmd.arg(&file);
        cmd.arg("-i");
        cmd.env("HOME", &home);
        cmd.cwd(&home);
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        writer
            .write_all(b"cd /\nfalse\necho 'a;b'\n\nexit\n")
            .unwrap();
        // All it writes, until it ends (the read fails then, which is fine).
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let _ = reader.read_to_end(&mut out);
            let _ = send.send(out);
        });
        let out = receive
            .recv_timeout(Duration::from_secs(20))
            .expect("bash didn't end");
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&home);
        let out = String::from_utf8_lossy(&out);

        let start = |text: &str| format!("\x1b]633;E;{text}\x1b\\\x1b]133;C\x1b\\");
        assert!(out.contains(&start("cd /")), "{out:?}");
        assert!(
            out.contains("\x1b]133;D;0\x1b\\\x1b]9;9;/\x1b\\"),
            "{out:?}"
        );
        assert!(out.contains(&start("false")), "{out:?}");
        assert!(out.contains("\x1b]133;D;1\x1b\\"), "{out:?}");
        assert!(out.contains(&start("echo 'a\\x3bb'")), "{out:?}");
        // The empty line started nothing: four commands, `exit` the last.
        assert_eq!(out.matches("\x1b]133;C").count(), 4, "{out:?}");
    }

    #[test]
    fn powershell_script_has_no_double_quotes() {
        assert!(!POWERSHELL.contains('"'));
    }
}
