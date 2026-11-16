// How good is a local model at "I forgot the command"? What `ask` (Ctrl+Shift+A)
// would put in the shell's line, for a list of questions:
// cargo run --example probe_ai -- qwen2.5-coder:7b llama3.1:8b
//
// Each question of examples/probe_ai.txt goes to each model as Kel asks it
// (`ask::answer`: the shell, the system, the folder told), through Ollama.
// The command, and the time it took. Whether it's right is for a person to say.
//
// Measured on 24 questions (2026-11-16, an RTX 4050 with 6 GB): qwen2.5-coder:7b
// 19 to 20 right, ~1 s each; llama3.1:8b 16 to 19, ~1 s, with things nobody asked for
// (`; Remove-Item .\archive.zip`); qwen3.5:9b 15 to 17, ~2.5 s (too big for the
// card), commands that don't exist (`Rename-Object`). The questions in Russian
// or in English: the same.
use kel_terminal::ask::{self, Ai};
use std::time::{Duration, Instant};

fn main() {
    let mut models: Vec<String> = std::env::args().skip(1).collect();
    if models.is_empty() {
        models.push(Ai::default().model);
    }
    let text = std::fs::read_to_string("examples/probe_ai.txt").expect("examples/probe_ai.txt");
    let questions: Vec<(&str, &str)> = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once(": "))
        .collect();
    let folder = std::env::current_dir().ok();

    for model in models {
        println!("\n== {model}");
        let ai = Ai {
            model: model.clone(),
            ..Ai::default()
        };
        let mut total = Duration::ZERO;
        for (shell, question) in &questions {
            let start = Instant::now();
            let answer = ask::answer(&ai, shell, folder.as_deref(), question);
            let time = start.elapsed();
            total += time;
            println!("\n{shell}: {question}");
            match answer {
                Ok(command) => println!("  > {command}"),
                Err(why) => println!("  ! {why}"),
            }
            println!("  {:.1} s", time.as_secs_f64());
        }
        println!(
            "\n{model}: {} questions, {:.0} s",
            questions.len(),
            total.as_secs_f64()
        );
    }
}
