// Temporary: records what nvim sends through ConPTY. cargo run --example probe_nvim
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let pair = native_pty_system().openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new("nvim");
    cmd.args(["--clean", "src/app.rs"]);
    cmd.cwd(std::env::current_dir()?);
    let _child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    writer.write_all(b"\x1b[1;1R")?;

    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            println!("{:?}", String::from_utf8_lossy(&buf[..n]));
        }
    });
    std::thread::sleep(Duration::from_secs(3));
    println!("===== Ctrl+E x3 (scroll down 3 lines) =====");
    writer.write_all(b"\x05\x05\x05")?;
    std::thread::sleep(Duration::from_secs(1));
    for (name, keys) in [
        ("dd", &b"jjjdd"[..]),
        ("x x x", b"xxx"),
        ("insert hello", b"ihello\x1b"),
        ("o new line", b"onew line\x1b"),
    ] {
        println!("===== {name} =====");
        writer.write_all(keys)?;
        std::thread::sleep(Duration::from_millis(700));
    }
    println!("===== typing :qa! =====");
    writer.write_all(b"\x1b:qa!\r")?;
    std::thread::sleep(Duration::from_secs(2));
    Ok(())
}
