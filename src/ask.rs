//! Ask for a command (Ctrl+Shift+A): a box over the terminal's bottom. Type what
//! the command should do, Enter, and a language model on this computer (see
//! `ollama`) writes it into the shell's line. It isn't run: read it, then Enter
//! is yours. A small model is right about four times in five (measured,
//! `examples/probe_ai.rs`), and a wrong command looks as sure as a right one.
//!
//! The model is told what the terminal knows by itself: the tab's shell, the
//! system, the folder. So the answer is in that shell's words, which a chat in
//! a browser has to be told each time.
//!
//! Here: the box's state, the question as the model gets it, and the command
//! taken out of its answer. `App` sends the keys and asks on a thread of its
//! own, `renderer` draws.

use crate::ollama;
use serde::Deserialize;
use std::path::Path;

/// `[ai]` in the config.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Ai {
    /// The model, by Ollama's name for it (`ollama list`).
    pub model: String,
    /// Where Ollama answers: this computer, its usual port.
    pub address: String,
}

impl Default for Ai {
    fn default() -> Self {
        Ai {
            // The best of the three tried that fit a 6 GB graphics card.
            model: "qwen2.5-coder:7b".to_string(),
            address: "127.0.0.1:11434".to_string(),
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

/// The command for `question`, from the model: asked, and taken out of what it
/// said. `shell` as the config has it (`pwsh.exe -NoLogo`), `folder` the tab's.
/// It waits for the answer: seconds, so not on the main thread.
pub fn answer(
    ai: &Ai,
    shell: &str,
    folder: Option<&Path>,
    question: &str,
) -> Result<String, String> {
    let messages = messages(shell, folder, question);
    let messages: Vec<(&str, &str)> = messages
        .iter()
        .map(|(role, text)| (*role, text.as_str()))
        .collect();
    let said = ollama::chat(&ai.address, &ai.model, &messages)?;
    command(&said).ok_or_else(|| "the model gave no command".to_string())
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
/// for and where it is, an example with its answer, the question.
fn messages(shell: &str, folder: Option<&Path>, question: &str) -> Vec<(&'static str, String)> {
    let told = told(shell);
    let folder = match folder {
        Some(folder) => format!(" Current folder: {}.", folder.display()),
        None => String::new(),
    };
    let rules = format!(
        "You are a helper inside a terminal. Shell: {}. System: {}.{folder} Answer \
         with one command for this shell that does what is asked, on a single line. \
         Only the command: no explanation, no markdown, no backticks. Paths relative \
         to the current folder, not full ones.",
        told.shell, told.system,
    );
    vec![
        ("system", rules),
        ("user", EXAMPLE.to_string()),
        ("assistant", told.example.to_string()),
        ("user", question.to_string()),
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

    #[test]
    fn each_shell_in_its_own_words() {
        let rules = |shell| messages(shell, None, "q").remove(0).1;
        let example = |shell| messages(shell, None, "q").remove(2).1;
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
        let messages = messages("bash", Some(&folder), "unpack a.tar.gz");
        let roles: Vec<&str> = messages.iter().map(|(role, _)| *role).collect();
        assert_eq!(roles, ["system", "user", "assistant", "user"]);
        let folder = format!("Current folder: {}.", folder.display());
        assert!(messages[0].1.contains(&folder), "{}", messages[0].1);
        assert_eq!(messages[1].1, EXAMPLE);
        assert_eq!(messages[3].1, "unpack a.tar.gz");
        // A shell that says no folder: none named.
        let rules = &super::messages("bash", None, "q")[0].1;
        assert!(!rules.contains("Current folder"), "{rules}");
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
    }
}
