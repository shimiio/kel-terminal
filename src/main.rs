use kel_terminal::app::{App, UserEvent};
use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    // with_user_event lets other threads wake the loop with our own UserEvent
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut app = App::new(event_loop.create_proxy())?;
    event_loop.run_app(&mut app)?; // never returns until the window closes
    Ok(())
}
