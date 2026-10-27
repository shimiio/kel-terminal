// cargo run --example dump_bytes
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};

fn main() -> anyhow::Result<()> {
    let pair = native_pty_system().openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-Command",
        "Write-Host 'hello' -ForegroundColor Red; Write-Host 'world' -ForegroundColor Green",
    ]);
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
    std::thread::sleep(std::time::Duration::from_secs(4));
    Ok(())
}
