// How good is a local model at "I forgot the command"? What `ask` (Ctrl+Shift+A)
// would put in the shell's line, for a list of questions:
// cargo run --example probe_ai -- qwen2.5-coder:7b llama3.1:8b
//
// Each question of examples/probe_ai.txt goes to each model as Kel asks it
// (`ask::answer`), through Ollama: the shell told, this folder with its files,
// and the commands the list says ran before; the answer checked by PowerShell,
// and asked once more if something in it isn't there. The command, the one it
// was before the check refused it, and the time it took. Whether it's right is
// for a person to say.
// `--bare`: only the shell told, to see what the rest is worth.
// `--no-check`: the first answer, as it is.
//
// Measured on 24 questions (2026-11-16, an RTX 4050 with 6 GB): qwen2.5-coder:7b
// 19 to 20 right, ~1 s each; llama3.1:8b 16 to 19, ~1 s, with things nobody
// asked for (`; Remove-Item .\archive.zip`); qwen3.5:9b 15 to 17, ~2.5 s (too
// big for the card), commands that don't exist (`Rename-Object`). The questions
// in Russian or in English: the same.
// With the folder's files and the last commands told (qwen2.5-coder:7b): the 8
// questions about them ("build this project", "the same for the branch dev")
// 8 right, 0 with `--bare`; of the first 24 one went wrong that was right
// ("files changed in the last 2 days" became `git diff`, in a folder with .git).
// With the check: `Select-String ... -Recurse` (no such parameter) came back
// as `Get-ChildItem -Recurse -Filter *.rs | Select-String ...`; a PowerShell
// answer takes 0.3 s longer. One question isn't the same each run, check or
// not: "bigger than 100 MB" is `100MB` in some, `1GB` in others.
use kel_terminal::ask::{self, Ai, Around};
use std::time::{Duration, Instant};

/// A question of the list: the shell it's asked in, the commands that ran
/// before it (`false`: it failed), the question.
struct Question {
    shell: String,
    commands: Vec<(String, Option<bool>)>,
    text: String,
}

/// The list: `shell: question` a line; before it, `ran: git status` or
/// `failed: cargo biuld` for each command the tab ran. `#`: a comment.
fn questions(list: &str) -> Vec<Question> {
    let mut questions = Vec::new();
    let mut commands = Vec::new();
    for line in list.lines().filter(|line| !line.starts_with('#')) {
        let Some((first, rest)) = line.split_once(": ") else {
            continue;
        };
        match first {
            "ran" => commands.push((rest.to_string(), Some(true))),
            "failed" => commands.push((rest.to_string(), Some(false))),
            shell => questions.push(Question {
                shell: shell.to_string(),
                commands: std::mem::take(&mut commands),
                text: rest.to_string(),
            }),
        }
    }
    questions
}

fn main() {
    let mut models: Vec<String> = std::env::args().skip(1).collect();
    let bare = models.iter().any(|arg| arg == "--bare");
    let check = !models.iter().any(|arg| arg == "--no-check");
    models.retain(|arg| !arg.starts_with("--"));
    if models.is_empty() {
        models.push(Ai::default().model);
    }
    let list = std::fs::read_to_string("examples/probe_ai.txt").expect("examples/probe_ai.txt");
    let questions = questions(&list);
    let folder = std::env::current_dir().ok();

    for model in models {
        println!("\n== {model}{}", if bare { ", bare" } else { "" });
        let ai = Ai {
            model: model.clone(),
            check,
            ..Ai::default()
        };
        let mut total = Duration::ZERO;
        let (mut second, mut doubted) = (0, 0);
        for question in &questions {
            let around = if bare {
                Around {
                    shell: question.shell.clone(),
                    ..Around::default()
                }
            } else {
                Around {
                    shell: question.shell.clone(),
                    folder: folder.clone(),
                    commands: question.commands.clone(),
                }
            };
            let start = Instant::now();
            let answer = ask::answer(&ai, &around, &question.text);
            let time = start.elapsed();
            total += time;
            println!();
            for (line, ok) in &question.commands {
                let how = if *ok == Some(false) { "failed" } else { "ran" };
                println!("{how}: {line}");
            }
            println!("{}: {}", question.shell, question.text);
            match answer {
                Ok(answer) => {
                    if let Some(was) = &answer.was {
                        second += 1;
                        println!("  was {was}");
                    }
                    println!("  > {}", answer.command);
                    if let Some(doubt) = &answer.doubt {
                        doubted += 1;
                        println!("  ? {doubt}");
                    }
                }
                Err(why) => println!("  ! {why}"),
            }
            println!("  {:.1} s", time.as_secs_f64());
        }
        println!(
            "\n{model}: {} questions, {:.0} s; {second} asked twice, {doubted} still doubted",
            questions.len(),
            total.as_secs_f64()
        );
    }
}
