use portable_pty::{CommandBuilder, PtySize, native_pty_system};

fn main() {
    let pty_system = native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("failed to create PTY");
    println!("invisible PTY created");

    // which shell to run
    let shell = if cfg!(windows) { "cmd.exe" } else { "bush" };

    // command builder for shell
    let cmd = CommandBuilder::new(shell);
    println!("spawning {}", shell);

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .expect("failed to spawn shell");

    // get pid
    if let Some(pid) = child.process_id() {
        println!("pid: {}", pid);
    }

    child.kill().unwrap();
    println!("shell killed")
}
