//! `shell` in the config: the program Kel starts in a tab, with its arguments.
//!
//! ```toml
//! shell = "pwsh.exe -NoLogo"
//! shell = '"C:\Program Files\PowerShell\7\pwsh.exe" -NoLogo'   # spaces: in ""
//! ```
//!
//! One line, as you'd type it: words split at spaces, a part in `"` is one word.
//! Without the setting: the system's shell.

/// The shell when the config names none: PowerShell on Windows (every Windows
/// has it); elsewhere yours, `$SHELL` (bash, zsh, fish...), or `sh` without one.
pub fn system() -> String {
    if cfg!(windows) {
        return "powershell.exe".to_string();
    }
    std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_string())
}

/// `shell` as the program and its arguments:
/// `pwsh.exe -NoLogo` → `pwsh.exe`, `[-NoLogo]`.
///
/// Without any `"`, the program goes up to `.exe`, spaces and all: before
/// arguments, `shell = 'C:\Program Files\PowerShell\7\pwsh.exe'` worked without
/// quotes, and still does.
pub fn split(shell: &str) -> (String, Vec<String>) {
    let shell = shell.trim();
    if !shell.contains('"') {
        // Lowercase has the same byte length for ASCII: a place in it is one in `shell`.
        let lower = shell.to_ascii_lowercase();
        let ends = |end: usize| shell[end..].chars().next().is_none_or(char::is_whitespace);
        let program_end = lower
            .match_indices(".exe")
            .map(|(i, _)| i + 4)
            .find(|&end| ends(end));
        if let Some(end) = program_end {
            return (shell[..end].to_string(), words(&shell[end..]));
        }
    }
    let mut words = words(shell).into_iter();
    (words.next().unwrap_or_default(), words.collect())
}

/// `text` in words: split at spaces, a part in `"` one word (`""` an empty one).
fn words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    // `""` is a word too, empty: `in_word` says one is started.
    let (mut quoted, mut in_word) = (false, false);
    for c in text.chars() {
        match c {
            '"' => (quoted, in_word) = (!quoted, true),
            c if c.is_whitespace() && !quoted => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shell_and_its_arguments() {
        let split = |s| {
            let (program, args) = split(s);
            (program, args.join("|"))
        };
        assert_eq!(
            split("powershell.exe"),
            ("powershell.exe".into(), "".into())
        );
        assert_eq!(
            split("  pwsh.exe   -NoLogo "),
            ("pwsh.exe".into(), "-NoLogo".into())
        );
        assert_eq!(
            split("wsl.exe -d Ubuntu"),
            ("wsl.exe".into(), "-d|Ubuntu".into())
        );
        // A path with spaces, in quotes; backslashes stay.
        assert_eq!(
            split(r#""C:\Program Files\PowerShell\7\pwsh.exe" -NoLogo"#),
            (
                r"C:\Program Files\PowerShell\7\pwsh.exe".into(),
                "-NoLogo".into()
            )
        );
        // An argument with spaces, and an empty one.
        assert_eq!(
            split(r#"cmd.exe /k "echo hi there" """#),
            ("cmd.exe".into(), "/k|echo hi there|".into())
        );
        assert_eq!(split(""), ("".into(), "".into()));
    }

    #[test]
    fn a_path_with_spaces_without_quotes_still_works() {
        let pwsh = r"C:\Program Files\PowerShell\7\pwsh.exe";
        assert_eq!(split(pwsh), (pwsh.to_string(), Vec::new()));
        assert_eq!(
            split(&format!("{pwsh} -NoLogo")),
            (pwsh.to_string(), vec!["-NoLogo".to_string()])
        );
        // ".exe" inside a name isn't its end.
        assert_eq!(split("a.exec b").0, "a.exec");
        assert_eq!(split(r"C:\my.exec\sh.exe -l").0, r"C:\my.exec\sh.exe");
    }

    #[test]
    fn the_system_s_shell() {
        if cfg!(windows) {
            assert_eq!(system(), "powershell.exe");
        } else {
            assert!(!system().is_empty());
        }
    }
}
