// A window program, not a console one: a shell that starts `kel` doesn't wait
// for it to close, and Explorer doesn't open a black console window next to it.
// Debug builds (`cargo run`) stay console ones, to show panics and `dbg!`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kel_terminal::app::{App, UserEvent};
use kel_terminal::project;
use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    // with_user_event lets other threads wake the loop with our own UserEvent
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    // On a Mac, no menu of winit's: its Cmd+Q would quit at once, without
    // saving the project or asking about a running command. Kel has its own
    // Cmd+Q, Cmd+H and Cmd+M (see `keys::mac_shortcut`).
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_default_menu(false);
    }
    let event_loop = builder.build()?;
    // `kel <name>` opens the project <name>; `kel <folder>` (Explorer's "Open
    // in Kel") a tab there, or the project of that folder (see `project::launch`).
    let launch = project::launch(std::env::args().nth(1));
    let mut app = App::new(event_loop.create_proxy(), launch)?;
    event_loop.run_app(&mut app)?; // never returns until the window closes
    Ok(())
}
