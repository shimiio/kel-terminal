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
//! - **zsh** (macOS' shell, Linux): zsh has no `--rcfile`, but it looks for its
//!   files (`.zshenv`, `.zshrc`) in the folder `ZDOTDIR` names. Kel names its own,
//!   with a `.zshenv` (`ZSH`) that puts `ZDOTDIR` back, reads yours, and adds the
//!   hooks; zsh reads the rest of your files from your folder, as always.
//! - Other shells: nothing; Kel just doesn't know their folder.

use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};

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

/// zsh's `.zshenv` in Kel's `ZDOTDIR`, the first file zsh reads:
///
/// - `ZDOTDIR` back to what it was (`KEL_ZDOTDIR`, set by Kel if it had one),
///   or none: your home. zsh looks it up before each file, so your `.zprofile`
///   and `.zshrc` come from there. Then your `.zshenv`.
/// - `__kel_precmd`, before each prompt: "the command ended", and the folder.
///   Added before your `.zshrc` adds its own (oh-my-zsh, powerlevel10k): the
///   first, `$?` still the command's.
/// - `__kel_preexec`, on Enter: zsh gives it the line as typed, its text then
///   "the command starts". zsh doesn't call it for an empty line.
///
/// `'\'` in quotes for the same reason as in `BASH`; `code`, not `status`,
/// which zsh keeps for itself.
const ZSH: &str = r#"# Kel's shell integration for zsh. Kel writes this file at each start: a change
# here doesn't stay. Kel started zsh with ZDOTDIR here, so zsh read this first:
# ZDOTDIR back to yours (none: your home), and your .zshenv.
if [[ -n "${KEL_ZDOTDIR+x}" ]]; then ZDOTDIR=$KEL_ZDOTDIR; else unset ZDOTDIR; fi
unset KEL_ZDOTDIR
if [[ -f "${ZDOTDIR:-$HOME}/.zshenv" ]]; then source "${ZDOTDIR:-$HOME}/.zshenv"; fi

if [[ -o interactive ]]; then
  __kel_precmd() {
    local code=$?
    printf '\e]133;D;%s\e\\\e]9;9;%s\e\\' "$(( code != 0 ))" "$PWD"
  }

  __kel_preexec() {
    local text=$1 bs='\x5c' sc='\x3b' nl='\x0a' tab='\x09' esc='\x1b' bel='\x07'
    text=${text//'\'/"$bs"}
    text=${text//;/"$sc"}
    text=${text//$'\n'/"$nl"}
    text=${text//$'\t'/"$tab"}
    text=${text//$'\e'/"$esc"}
    text=${text//$'\a'/"$bel"}
    printf '\e]633;E;%s\e\\\e]133;C\e\\' "$text"
  }

  autoload -Uz add-zsh-hook
  add-zsh-hook precmd __kel_precmd
  add-zsh-hook preexec __kel_preexec
fi
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
            if let Some(file) = written("kel.bash", BASH) {
                cmd.arg("--rcfile");
                cmd.arg(file);
            }
            cmd.args(args);
        }
        Some("zsh") if cfg!(unix) => {
            if let Some(dir) = written("zsh/.zshenv", ZSH)
                .as_deref()
                .and_then(Path::parent)
            {
                match std::env::var_os("ZDOTDIR") {
                    Some(yours) => cmd.env("KEL_ZDOTDIR", yours),
                    None => cmd.env_remove("KEL_ZDOTDIR"),
                }
                cmd.env("ZDOTDIR", dir);
            }
            cmd.args(args);
        }
        _ => cmd.args(args),
    }
    cmd.env("PROMPT", cmd_prompt(std::env::var("PROMPT").ok()));
}

/// The file `name` in Kel's folder's `shell`, e.g. `~/.config/kel/shell/kel.bash`,
/// with `text` in it: written if it isn't so (the first time, or a newer Kel's
/// text). `None` if it can't be: the shell starts without integration.
fn written(name: &str, text: &str) -> Option<PathBuf> {
    let file = crate::config::dir().ok()?.join("shell").join(name);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(text) {
        std::fs::create_dir_all(file.parent()?).ok()?;
        std::fs::write(&file, text).ok()?;
    }
    Some(file)
}

/// Whether `shell` reports its folder at each prompt, so Kel can tell when it's
/// ready for typing (see `Tab::type_at_start`).
pub fn reports_folder(shell: &str) -> bool {
    let name = name(shell);
    let unix = cfg!(unix) && matches!(name.as_deref(), Some("bash" | "zsh"));
    unix || matches!(name.as_deref(), Some("powershell" | "pwsh" | "cmd"))
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

    /// Runs `cmd` (a shell) in a pseudo-terminal, as a tab does, types
    /// `input`, and gives back all it wrote until it ended.
    #[cfg(unix)]
    fn run_in_pty(cmd: CommandBuilder, input: &[u8]) -> String {
        use portable_pty::{PtySize, native_pty_system};
        use std::io::{Read, Write};
        use std::time::Duration;

        let size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = native_pty_system().openpty(size).unwrap();
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        writer.write_all(input).unwrap();
        // All it writes, until it ends (the read fails then, which is fine).
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let _ = reader.read_to_end(&mut out);
            let _ = send.send(out);
        });
        let out = receive.recv_timeout(Duration::from_secs(20));
        let _ = child.kill();
        String::from_utf8_lossy(&out.expect("the shell didn't end")).into_owned()
    }

    /// A new empty folder for a test, as its `HOME`: not the machine's files.
    #[cfg(unix)]
    fn test_home(name: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("kel-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        home
    }

    /// What both shells must have said for a first `echo` (each test's own),
    /// `cd /`, `false`, `echo 'a;b'`, an empty line and `exit`.
    #[cfg(unix)]
    fn check_reports(out: &str) {
        let start = |text: &str| format!("\x1b]633;E;{text}\x1b\\\x1b]133;C\x1b\\");
        assert!(out.contains(&start("cd /")), "{out:?}");
        assert!(
            out.contains("\x1b]133;D;0\x1b\\\x1b]9;9;/\x1b\\"),
            "{out:?}"
        );
        assert!(out.contains(&start("false")), "{out:?}");
        assert!(out.contains("\x1b]133;D;1\x1b\\"), "{out:?}");
        assert!(out.contains(&start("echo 'a\\x3bb'")), "{out:?}");
        // The empty line started nothing: five commands, `exit` the last.
        assert_eq!(out.matches("\x1b]133;C").count(), 5, "{out:?}");
    }

    #[cfg(unix)]
    const INPUT: &[u8] = b"cd /\nfalse\necho 'a;b'\n\nexit\n";

    /// The hooks in a real bash: what it says, and your `~/.bashrc` still read.
    #[cfg(unix)]
    #[test]
    fn bash_reports_folders_and_commands() {
        let home = test_home("bash");
        std::fs::write(home.join(".bashrc"), "export FROM_BASHRC=yes\n").unwrap();
        let file = home.join("kel.bash");
        std::fs::write(&file, BASH).unwrap();
        let mut cmd = CommandBuilder::new("bash");
        cmd.arg("--rcfile");
        cmd.arg(&file);
        cmd.arg("-i");
        cmd.env("HOME", &home);
        cmd.cwd(&home);
        let input = [b"echo $FROM_BASHRC-from\n", INPUT].concat();
        let out = run_in_pty(cmd, &input);
        let _ = std::fs::remove_dir_all(&home);
        check_reports(&out);
        assert!(out.contains("yes-from"), "{out:?}");
    }

    /// The hooks in a real zsh, if there is one (macOS has it, Ubuntu not
    /// always): what it says, your `.zshenv` and `.zshrc` still read, and
    /// `ZDOTDIR` yours again.
    #[cfg(unix)]
    #[test]
    fn zsh_reports_folders_and_commands() {
        if std::process::Command::new("zsh")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let home = test_home("zsh");
        std::fs::write(home.join(".zshenv"), "export FROM_ZSHENV=yes\n").unwrap();
        std::fs::write(home.join(".zshrc"), "export FROM_ZSHRC=yes\n").unwrap();
        let kel = home.join("kel");
        std::fs::create_dir_all(&kel).unwrap();
        std::fs::write(kel.join(".zshenv"), ZSH).unwrap();
        let mut cmd = CommandBuilder::new("zsh");
        cmd.arg("-i");
        cmd.env("HOME", &home);
        cmd.env("ZDOTDIR", &kel);
        cmd.env_remove("KEL_ZDOTDIR");
        cmd.cwd(&home);
        let input = [
            b"echo $FROM_ZSHENV-$FROM_ZSHRC-${ZDOTDIR-none}-from\n",
            INPUT,
        ]
        .concat();
        let out = run_in_pty(cmd, &input);
        let _ = std::fs::remove_dir_all(&home);
        check_reports(&out);
        assert!(out.contains("yes-yes-none-from"), "{out:?}");
    }

    #[test]
    fn powershell_script_has_no_double_quotes() {
        assert!(!POWERSHELL.contains('"'));
    }
}
