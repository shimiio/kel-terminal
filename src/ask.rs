//! Ask for a command (Ctrl+Shift+A): a box over the terminal's bottom. Type what
//! the command should do, Enter, and a language model on this computer (see
//! `ollama`) writes it into the shell's line. It isn't run: read it, then Enter
//! is yours. A small model is right about four times in five (measured,
//! `examples/probe_ai.rs`), and a wrong command looks as sure as a right one.
//!
//! The model is told what the terminal knows by itself (see `Around`): the
//! tab's shell, the system, the folder and its files' names, the last commands
//! run there and which failed. So the answer is in that shell's words, "build
//! this project" is `cargo build` here, and "the same for the branch dev"
//! means something: what a chat in a browser has to be told each time. Each
//! question is on its own: the one before isn't remembered.
//!
//! In PowerShell the answer is checked before it goes into the line (see
//! `check`): a command or a parameter that isn't there goes back to the model,
//! once.
//!
//! Here: the box's state, the question as the model gets it, and the command
//! taken out of its answer. `App` sends the keys and asks on a thread of its
//! own, `renderer` draws.

use crate::check::{self, Finding};
use crate::ollama;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// `[ai]` in the config.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Ai {
    /// The model, by Ollama's name for it (`ollama list`).
    pub model: String,
    /// Where Ollama answers: this computer, its usual port.
    pub address: String,
    /// PowerShell looks at a command before it goes into the line (see `check`).
    pub check: bool,
}

impl Default for Ai {
    fn default() -> Self {
        Ai {
            // The best of the three tried that fit a 6 GB graphics card.
            model: "qwen2.5-coder:7b".to_string(),
            address: "127.0.0.1:11434".to_string(),
            check: true,
        }
    }
}

/// The box, while it's open.
#[derive(Default)]
pub struct Ask {
    /// What's typed in it.
    query: String,
    state: State,
}

#[derive(Default)]
enum State {
    #[default]
    Typing,
    /// Asked, the answer isn't here yet: the question's number (see `sent`).
    Waiting(u64),
    /// No answer, and why. Typing goes on from here.
    Failed(String),
}

impl Ask {
    pub fn new() -> Ask {
        Ask::default()
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn type_text(&mut self, text: &str) {
        self.query.push_str(text);
        self.state = State::Typing;
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.state = State::Typing;
    }

    /// The question went to the model, as number `number`: each one has its
    /// own, so an answer that comes after Esc and a new question isn't taken
    /// for the new one's.
    pub fn sent(&mut self, number: u64) {
        self.state = State::Waiting(number);
    }

    /// The model is asked: the box takes no typing until it answers.
    pub fn waiting(&self) -> bool {
        matches!(self.state, State::Waiting(_))
    }

    /// Whether the answer to question `number` is the one waited for.
    pub fn waits_for(&self, number: u64) -> bool {
        matches!(self.state, State::Waiting(n) if n == number)
    }

    /// No answer came: the box stays, saying why.
    pub fn failed(&mut self, why: String) {
        self.state = State::Failed(why);
    }

    pub fn error(&self) -> Option<&str> {
        match &self.state {
            State::Failed(why) => Some(why),
            _ => None,
        }
    }
}

/// What's around the question, which the terminal knows and a chat in a browser
/// has to be told: the tab it's asked in.
#[derive(Clone, Debug, Default)]
pub struct Around {
    /// The tab's shell, as the config has it: `pwsh.exe -NoLogo`.
    pub shell: String,
    /// The shell's folder, if it said one. Its files are told too, by name:
    /// `Cargo.toml` or `package.json` say what kind of project this is.
    pub folder: Option<PathBuf>,
    /// The last commands run in the tab, oldest first, each with how it ended
    /// if the shell said (`false`: it failed). So "the same for the branch dev"
    /// is a question, and "why" has something to be about.
    pub commands: Vec<(String, Option<bool>)>,
}

/// At most this many of a folder's files are named: a few lines of the
/// question, and a home folder has hundreds.
const FILES: usize = 40;

/// The names in `folder`, folders with a `/` after them, in the alphabet's
/// order: the first `FILES`, and how many more there are. A folder that can't
/// be read (WSL's, as Windows sees its path) has none.
fn files_in(folder: &Path) -> (Vec<String>, usize) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return (Vec::new(), 0);
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => format!("{name}/"),
                _ => name,
            }
        })
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    let more = names.len().saturating_sub(FILES);
    names.truncate(FILES);
    (names, more)
}

/// What the model answered.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    /// The command for the shell's line.
    pub command: String,
    /// Its first answer, if the check refused it and `command` is the second,
    /// which the check let through.
    pub was: Option<String>,
    /// What the check finds in `command`, the first answer, when the second
    /// try was no better: it goes into the line all the same, with this said.
    pub doubt: Option<String>,
}

/// The command for `question`, from the model: asked, taken out of what it
/// said, and checked (see `check`); with something wrong in it, asked once
/// more. It reads the folder and waits for the model and for PowerShell:
/// seconds, so not on the main thread.
pub fn answer(ai: &Ai, around: &Around, question: &str) -> Result<Answer, String> {
    let files = around.folder.as_deref().map(files_in).unwrap_or_default();
    let messages = messages(around, &files, question);
    let chat = |messages: &[(&str, &str)]| ollama::chat(&ai.address, &ai.model, messages);
    let check = |command: &str| match ai.check {
        true => check::check(&around.shell, around.folder.as_deref(), command),
        false => Vec::new(),
    };
    answer_with(chat, check, &messages)
}

/// `answer`, with `chat` the model and `check` the check. One second try, not
/// more: a small model that's wrong twice doesn't get it the third time, and
/// each try is a second of waiting.
fn answer_with(
    chat: impl Fn(&[(&str, &str)]) -> Result<String, String>,
    check: impl Fn(&str) -> Vec<Finding>,
    messages: &[(&'static str, String)],
) -> Result<Answer, String> {
    let nothing = || "the model gave no command".to_string();
    let mut messages: Vec<(&str, &str)> = messages
        .iter()
        .map(|(role, text)| (*role, text.as_str()))
        .collect();
    let first = command(&chat(&messages)?).ok_or_else(nothing)?;
    let found = check(&first);
    if found.is_empty() {
        return Ok(Answer {
            command: first,
            was: None,
            doubt: None,
        });
    }
    // Its own answer, and what's wrong with it, with what there is instead.
    let wrong: Vec<String> = found
        .iter()
        .map(|finding| format!("{}. {}", finding.what, finding.instead))
        .collect();
    let again = format!(
        "That command doesn't work in this shell:\n{}\nAnswer with the command \
         put right. Only the command.",
        wrong.join("\n").trim(),
    );
    messages.push(("assistant", &first));
    messages.push(("user", &again));
    // The second answer only if nothing is found in it. Else the first, with
    // what was found: a program that isn't installed (`ffmpeg`) is nothing the
    // model can put right, and trying, it spoiled a right command (measured:
    // `-t 00:00:30` became `-t 00:01:00`).
    let second = chat(&messages).ok().and_then(|said| command(&said));
    match second.filter(|second| *second != first && check(second).is_empty()) {
        Some(second) => Ok(Answer {
            command: second,
            was: Some(first),
            doubt: None,
        }),
        None => Ok(Answer {
            doubt: Some(found[0].what.clone()),
            command: first,
            was: None,
        }),
    }
}

/// What a shell is to the model.
struct Told {
    /// Its name, said so that the model stays out of another shell's habits.
    shell: String,
    system: &'static str,
    /// The answer to `EXAMPLE` in this shell.
    example: &'static str,
}

/// A question every shell has a short answer to: with it the model sees what
/// an answer looks like. A small model follows an example better than a rule.
const EXAMPLE: &str = "how many files are in this folder and its subfolders";

fn told(shell: &str) -> Told {
    let system = if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    };
    let name = crate::shell::name(shell).unwrap_or_else(|| "sh".to_string());
    let power = "(Get-ChildItem -Recurse -File).Count";
    let unix = "find . -type f | wc -l";
    let (shell, system, example) = match name.as_str() {
        // Windows' own: models write `&&` in it, which only the newer one has.
        "powershell" => (
            "Windows PowerShell 5.1 (it has no && and no ||; use ; instead)",
            system,
            power,
        ),
        "pwsh" => ("PowerShell 7", system, power),
        "cmd" => (
            "cmd.exe, the Windows Command Prompt",
            system,
            "dir /s /b /a-d | find /c /v \"\"",
        ),
        // Linux inside Windows: its default shell.
        "wsl" => ("bash", "Linux (WSL)", unix),
        other => (other, system, unix),
    };
    Told {
        shell: shell.to_string(),
        system,
        example,
    }
}

/// The question as the model gets it, each part a role and its text: what it's
/// for and where it is, an example with its answer, the question. `files`: the
/// folder's (see `files_in`).
fn messages(
    around: &Around,
    (files, more): &(Vec<String>, usize),
    question: &str,
) -> Vec<(&'static str, String)> {
    let told = told(&around.shell);
    let mut place = String::new();
    if let Some(folder) = &around.folder {
        place += &format!(" Current folder: {}.", folder.display());
    }
    if !files.is_empty() {
        place += &format!(" Files in it: {}", files.join(", "));
        place += &match more {
            0 => ".".to_string(),
            more => format!(", and {more} more."),
        };
        // What it costs: in a folder with `.git`, "files changed in the last 2
        // days" becomes a question about git (measured; telling the model that
        // most questions aren't about the files changed nothing).
    }
    // The commands go with the question, after the example: said before it,
    // "the same for the branch dev" was taken to mean the example's command
    // (measured), the last thing the model had seen.
    let mut asked = String::new();
    if !around.commands.is_empty() {
        let commands: Vec<String> = around
            .commands
            .iter()
            .map(|(line, ok)| match ok {
                Some(false) => format!("`{line}` (it failed)"),
                _ => format!("`{line}`"),
            })
            .collect();
        asked += "The last commands run in this terminal, oldest first: ";
        asked += &format!("{}.\n\n", commands.join(", "));
    }
    asked += question;
    let rules = format!(
        "You are a helper inside a terminal. Shell: {}. System: {}.{place} Answer \
         with one command for this shell that does what is asked, on a single line. \
         Only the command: no explanation, no markdown, no backticks. Paths relative \
         to the current folder, not full ones.",
        told.shell, told.system,
    );
    vec![
        ("system", rules),
        ("user", EXAMPLE.to_string()),
        ("assistant", told.example.to_string()),
        ("user", asked),
    ]
}

/// The command in what the model said: its first line with something in it. A
/// model told "no markdown" still wraps it in backticks now and then, or in a
/// block of code: those go. `None`: nothing was there.
fn command(said: &str) -> Option<String> {
    let line = said
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("```"))?;
    let line = line
        .strip_prefix('`')
        .and_then(|line| line.strip_suffix('`'))
        .unwrap_or(line);
    // Nothing that would be a key in the shell's line (Esc, Ctrl+C).
    let line: String = line.chars().filter(|c| !c.is_control()).collect();
    let line = line.trim();
    (!line.is_empty()).then(|| line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_in_an_answer() {
        assert_eq!(
            command("git reset --soft HEAD~1").as_deref(),
            Some("git reset --soft HEAD~1")
        );
        // In backticks, in a block, with a word after it.
        assert_eq!(command(" `ls -la`\n").as_deref(), Some("ls -la"));
        assert_eq!(
            command("```powershell\nGet-ChildItem -Recurse\n```\nThis lists all.").as_deref(),
            Some("Get-ChildItem -Recurse")
        );
        // Backticks inside stay: PowerShell's escape, bash's old `$(...)`.
        assert_eq!(command("echo `date`").as_deref(), Some("echo `date`"));
        // No keys hidden in it.
        assert_eq!(command("ls\x1b[A\x03").as_deref(), Some("ls[A"));
        assert_eq!(command("  \n\n"), None);
        assert_eq!(command("```\n```"), None);
    }

    /// The question in a tab with this shell, nothing else known.
    fn in_shell(shell: &str) -> Vec<(&'static str, String)> {
        let around = Around {
            shell: shell.to_string(),
            ..Around::default()
        };
        messages(&around, &(Vec::new(), 0), "q")
    }

    #[test]
    fn each_shell_in_its_own_words() {
        let rules = |shell| in_shell(shell).remove(0).1;
        let example = |shell| in_shell(shell).remove(2).1;
        assert!(rules("powershell.exe").contains("Windows PowerShell 5.1 (it has no &&"));
        assert!(
            rules(r#""C:\Program Files\PowerShell\7\pwsh.exe" -NoLogo"#).contains("PowerShell 7.")
        );
        assert_eq!(example("pwsh.exe"), "(Get-ChildItem -Recurse -File).Count");
        assert!(rules("cmd.exe").contains("cmd.exe, the Windows Command Prompt"));
        assert!(example("cmd.exe").starts_with("dir /s"));
        assert!(rules("wsl.exe -d Ubuntu").contains("Shell: bash. System: Linux (WSL)."));
        assert!(rules("/bin/zsh -l").contains("Shell: zsh."));
        assert_eq!(example("/usr/bin/fish"), "find . -type f | wc -l");
    }

    #[test]
    fn the_question_as_the_model_gets_it() {
        let folder = Path::new("projects").join("kel");
        let around = Around {
            shell: "bash".to_string(),
            folder: Some(folder.clone()),
            commands: vec![
                ("git status".to_string(), Some(true)),
                ("cargo biuld".to_string(), Some(false)),
                ("nvim .".to_string(), None),
            ],
        };
        let files = (vec!["Cargo.toml".to_string(), "src/".to_string()], 0);
        let messages = messages(&around, &files, "unpack a.tar.gz");
        let roles: Vec<&str> = messages.iter().map(|(role, _)| *role).collect();
        assert_eq!(roles, ["system", "user", "assistant", "user"]);
        let rules = &messages[0].1;
        let place = format!(
            "Current folder: {}. Files in it: Cargo.toml, src/. Answer",
            folder.display()
        );
        assert!(rules.contains(&place), "{rules}");
        assert_eq!(messages[1].1, EXAMPLE);
        // The commands with the question, after the example.
        assert_eq!(
            messages[3].1,
            "The last commands run in this terminal, oldest first: `git status`, \
             `cargo biuld` (it failed), `nvim .`.\n\nunpack a.tar.gz"
        );
        // More files than are named.
        let files = (vec!["a".to_string()], 7);
        let rules = &super::messages(&around, &files, "q")[0].1;
        assert!(rules.contains("Files in it: a, and 7 more."), "{rules}");
        // A shell that says no folder, a tab where nothing ran: neither named.
        let rules = &in_shell("bash")[0].1;
        assert!(!rules.contains("Current folder"), "{rules}");
        assert!(!rules.contains("Files in it"), "{rules}");
        assert_eq!(in_shell("bash")[3].1, "q");
    }

    /// A model that says `answers` one after the other, and a check that
    /// refuses any command with `-Name` in it: the answer, and what the model
    /// was asked the last time.
    fn asked(answers: &[&str]) -> (Result<Answer, String>, Vec<(String, String)>) {
        use std::cell::RefCell;
        let left = RefCell::new(answers.to_vec());
        let last = RefCell::new(Vec::new());
        let chat = |messages: &[(&str, &str)]| {
            *last.borrow_mut() = messages
                .iter()
                .map(|(role, text)| (role.to_string(), text.to_string()))
                .collect();
            let mut left = left.borrow_mut();
            match left.is_empty() {
                true => Err("no Ollama".to_string()),
                false => Ok(left.remove(0).to_string()),
            }
        };
        let check = |command: &str| match command.contains("-Name") {
            true => vec![Finding {
                what: "Remove-Item has no parameter -Name".to_string(),
                instead: "Its parameters: -Path, -Force".to_string(),
            }],
            false => Vec::new(),
        };
        let answer = answer_with(chat, check, &in_shell("powershell.exe"));
        (answer, last.into_inner())
    }

    #[test]
    fn a_right_answer_is_asked_once() {
        let (answer, last) = asked(&["Remove-Item a.txt", "never asked for"]);
        let answer = answer.unwrap();
        assert_eq!(answer.command, "Remove-Item a.txt");
        assert_eq!((answer.was, answer.doubt), (None, None));
        assert_eq!(last.len(), 4);
    }

    #[test]
    fn a_wrong_answer_gets_a_second_try() {
        let (answer, last) = asked(&["Remove-Item -Name a.txt", "`Remove-Item -Path a.txt`"]);
        let answer = answer.unwrap();
        assert_eq!(answer.command, "Remove-Item -Path a.txt");
        assert_eq!(answer.was.as_deref(), Some("Remove-Item -Name a.txt"));
        assert_eq!(answer.doubt, None);
        // The second time: its own answer, and what's wrong with it.
        assert_eq!(last.len(), 6);
        assert_eq!(
            last[4],
            ("assistant".into(), "Remove-Item -Name a.txt".into())
        );
        assert_eq!(
            last[5].1,
            "That command doesn't work in this shell:\nRemove-Item has no parameter \
             -Name. Its parameters: -Path, -Force\nAnswer with the command put right. \
             Only the command."
        );
    }

    #[test]
    fn wrong_twice_goes_in_with_what_s_wrong() {
        // The first answer stays: the second is no better, and may be worse.
        let doubt = Some("Remove-Item has no parameter -Name".to_string());
        let twice = &["rm -Name a", "del -Name a.txt", "never"][..];
        // The same again, or no second answer at all: the first too.
        for answers in [
            twice,
            &["rm -Name a", "rm -Name a"][..],
            &["rm -Name a"][..],
        ] {
            let answer = asked(answers).0.unwrap();
            assert_eq!(answer.command, "rm -Name a");
            assert_eq!((answer.was, answer.doubt), (None, doubt.clone()));
        }
        // No first answer: that's why there's none.
        assert_eq!(asked(&[]).0, Err("no Ollama".to_string()));
        assert_eq!(
            asked(&["```\n```"]).0,
            Err("the model gave no command".to_string())
        );
    }

    #[test]
    fn a_folder_s_files() {
        let dir = std::env::temp_dir().join(format!("kel-ask-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        for name in ["b.txt", "Cargo.toml", ".gitignore"] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        let (names, more) = files_in(&dir);
        assert_eq!(names, [".gitignore", "b.txt", "Cargo.toml", "src/"]);
        assert_eq!(more, 0);
        // More than are named: the first ones, and how many are left.
        for n in 0..FILES {
            std::fs::write(dir.join(format!("z{n:02}")), "").unwrap();
        }
        let (names, more) = files_in(&dir);
        assert_eq!((names.len(), more), (FILES, 4));
        assert_eq!(names[3], "src/");
        std::fs::remove_dir_all(&dir).unwrap();
        // A folder that isn't there: none.
        assert_eq!(files_in(&dir), (Vec::new(), 0));
    }

    #[test]
    fn the_box() {
        let mut ask = Ask::new();
        ask.type_text("undo the last commit");
        assert!(!ask.waiting());
        ask.sent(3);
        assert!(ask.waiting() && ask.waits_for(3) && !ask.waits_for(2));
        ask.failed("no Ollama".to_string());
        assert_eq!(ask.error(), Some("no Ollama"));
        assert!(!ask.waiting() && !ask.waits_for(3));
        // Typing on: the error goes.
        ask.backspace();
        assert_eq!(ask.error(), None);
        assert_eq!(ask.query(), "undo the last commi");
    }

    #[test]
    fn the_config_s_ai() {
        use crate::config::Config;
        assert_eq!(Config::default().ai.model, "qwen2.5-coder:7b");
        let config: Config = toml::from_str("[ai]\nmodel = \"llama3.1:8b\"").unwrap();
        assert_eq!(config.ai.model, "llama3.1:8b");
        assert_eq!(config.ai.address, "127.0.0.1:11434");
        assert!(config.ai.check);
        let config: Config = toml::from_str("[ai]\ncheck = false").unwrap();
        assert!(!config.ai.check);
    }
}
