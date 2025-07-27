use app::App;
use winit::event_loop::EventLoop;

mod app;
mod font;

fn main() -> anyhow::Result<()> {
    let ev = EventLoop::new()?;
    ev.run_app(&mut App::default())?;
    Ok(())
}
