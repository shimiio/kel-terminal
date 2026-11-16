//! Is a command a model wrote made of things that exist? Asked of PowerShell,
//! without running the command. A small model writes `Remove-Item -Name x` as
//! surely as a right command, and PowerShell knows there's no `-Name`: every
//! one of its commands says which parameters it has (`Get-Command`), and its
//! parser cuts a line into commands and parameters without running anything.
//!
//! So before a command goes into the shell's line (see `ask`), a PowerShell of
//! the tab's kind, out of sight, looks at it: its syntax, each command's name,
//! each parameter's. What it finds goes back to the model, with the parameters
//! the command does have, for a second try.
//!
//! Only PowerShell's own commands: `git` and `cargo` keep their flags in a help
//! text, each its own way, and bash has nothing to ask. And only the form: 1 GB
//! where 100 MB was asked for is a right command. Measured on 34 commands models
//! gave (`examples/probe_ai.rs`): of 15 wrong ones 8 found, of 19 right ones
//! none taken for wrong, 0.3 s a check.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Something in a command that isn't there.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// What: `Remove-Item has no parameter -Name`.
    pub what: String,
    /// What there is instead, if PowerShell said: `Its parameters: -Path, ...`.
    pub instead: String,
}

/// PowerShell starts in 0.3 s; one that loads a big module for a command's
/// parameters takes a few more. Past this the command goes unchecked.
const PATIENCE: Duration = Duration::from_secs(6);

/// The script. Its lines out: what's wrong, a tab, what there is instead.
/// No double quotes in it: it goes to PowerShell as one argument, where a `"`
/// would end it (a test checks). The command comes in `$env:KEL_CHECK`, as
/// text: nothing here runs it.
const SCRIPT: &str = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$tab = [char]9
$tokens = $null
$errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseInput($env:KEL_CHECK, [ref]$tokens, [ref]$errors)
foreach ($e in $errors) { 'Syntax error' + $tab + $e.Message }
$common = [System.Management.Automation.PSCmdlet]::CommonParameters + [System.Management.Automation.PSCmdlet]::OptionalCommonParameters
foreach ($c in $ast.FindAll({ $args[0] -is [System.Management.Automation.Language.CommandAst] }, $true)) {
    $name = $c.GetCommandName()
    # Not a name: a wildcard would find every command there is.
    if (-not $name -or $name -match '[*?\[\]]') { continue }
    $cmd = Get-Command $name -ErrorAction Ignore | Select-Object -First 1
    if (-not $cmd) { 'There is no command ' + $name + ' here' + $tab; continue }
    if ($cmd.CommandType -eq 'Alias') { $cmd = $cmd.ResolvedCommand }
    # A program (git, cargo): its flags are its own, nothing to ask.
    if (-not $cmd -or ($cmd.CommandType -ne 'Cmdlet' -and $cmd.CommandType -ne 'Function')) { continue }
    $params = $cmd.Parameters
    if (-not $params) { continue }
    $names = @($params.Keys) + @($params.Values | ForEach-Object { $_.Aliases })
    foreach ($p in $c.CommandElements) {
        if ($p -isnot [System.Management.Automation.Language.CommandParameterAst]) { continue }
        $n = $p.ParameterName
        # PowerShell takes the start of a parameter's name: -Rec for -Recurse.
        if (@($names | Where-Object { $_ -like ($n + '*') }).Count -eq 0) {
            $has = @($params.Keys | Where-Object { $common -notcontains $_ } | ForEach-Object { '-' + $_ }) -join ', '
            $cmd.Name + ' has no parameter -' + $n + $tab + 'Its parameters: ' + $has
        }
    }
}
"#;

/// What's wrong with `command` for the shell `shell` (as the config has it),
/// run in `folder`. Empty: nothing found, or nothing to ask: not PowerShell,
/// it didn't start, it took too long. It waits for PowerShell: not on the main
/// thread.
pub fn check(shell: &str, folder: Option<&Path>, command: &str) -> Vec<Finding> {
    if !matches!(
        crate::shell::name(shell).as_deref(),
        Some("powershell" | "pwsh")
    ) {
        return Vec::new();
    }
    // The tab's own PowerShell: `&&` is wrong in Windows' 5.1 and right in 7.
    let mut powershell = Command::new(crate::shell::split(shell).0);
    powershell
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("KEL_CHECK", command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Where the command would run: which parameters `Get-ChildItem` has
    // depends on it (`-File` only among files).
    if let Some(folder) = folder.filter(|folder| folder.is_dir()) {
        powershell.current_dir(folder);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Kel has no console: without this one would open for it, and flash.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        powershell.creation_flags(CREATE_NO_WINDOW);
    }
    let Ok(mut child) = powershell.spawn() else {
        return Vec::new();
    };
    let Some(mut out) = child.stdout.take() else {
        return Vec::new();
    };
    // Read on a thread of its own, so the wait can end without it.
    let reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = out.read_to_end(&mut text);
        text
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < PATIENCE => {
                std::thread::sleep(Duration::from_millis(15));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Vec::new();
            }
        }
    }
    let text = reader.join().unwrap_or_default();
    findings(&String::from_utf8_lossy(&text))
}

/// The script's lines as findings.
fn findings(out: &str) -> Vec<Finding> {
    out.lines()
        .filter_map(|line| {
            let (what, instead) = line.split_once('\t').unwrap_or((line, ""));
            let what = what.trim();
            (!what.is_empty()).then(|| Finding {
                what: what.to_string(),
                instead: instead.trim().to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_s_lines() {
        let out = "Remove-Item has no parameter -Name\tIts parameters: -Path, -Force\r\n\
                   There is no command Rename-Object here\t\r\n\r\n";
        let found = findings(out);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].what, "Remove-Item has no parameter -Name");
        assert_eq!(found[0].instead, "Its parameters: -Path, -Force");
        assert_eq!(found[1].what, "There is no command Rename-Object here");
        assert_eq!(found[1].instead, "");
        assert_eq!(findings(""), Vec::new());
    }

    #[test]
    fn the_script_has_no_double_quotes() {
        assert!(!SCRIPT.contains('"'));
    }

    #[test]
    fn only_powershell_is_asked() {
        // Not started at all: bash has nothing to ask, nor cmd.
        assert_eq!(check("bash", None, "ls --nope"), Vec::new());
        assert_eq!(check("wsl.exe -d Ubuntu", None, "ls --nope"), Vec::new());
        assert_eq!(check("cmd.exe", None, "dir /nope"), Vec::new());
        // One that isn't there: unchecked, not an error.
        assert_eq!(
            check("no-such-dir/pwsh", None, "Get-Item -Nope"),
            Vec::new()
        );
    }

    /// The real one, where every Windows has it.
    #[cfg(windows)]
    #[test]
    fn powershell_says_what_isn_t_there() {
        let whats = |command: &str| -> Vec<String> {
            check("powershell.exe", None, command)
                .into_iter()
                .map(|finding| finding.what)
                .collect()
        };
        // Right ones, with a parameter cut short, one only files have, an
        // alias, a program with flags of its own.
        let right = "Get-ChildItem -Rec -File | Where-Object { $_.Length -gt 100MB }; \
                     gci -Path . | sort Length; cmd /c ver; \
                     Get-Content -Path a.log -Wait -Tail 20";
        assert_eq!(whats(right), Vec::<String>::new());
        // A parameter that isn't there, and the ones that are.
        let found = check("powershell.exe", None, "Remove-Item -Name \"new file.txt\"");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].what, "Remove-Item has no parameter -Name");
        assert!(
            found[0]
                .instead
                .starts_with("Its parameters: -Path, -LiteralPath")
        );
        assert!(
            !found[0].instead.contains("-Verbose"),
            "{}",
            found[0].instead
        );
        // A command that isn't there; nothing ran (the file would be there).
        let file = std::env::temp_dir().join(format!("kel-check-{}", std::process::id()));
        let command = format!("Set-Content '{}' x; Rename-Object a b", file.display());
        assert_eq!(whats(&command), ["There is no command Rename-Object here"]);
        assert!(!file.exists());
        // Windows' own PowerShell has no `&&`.
        assert_eq!(whats("git status && git log"), ["Syntax error"]);
    }
}
