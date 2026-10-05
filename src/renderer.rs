use std::num::NonZeroU32;
use std::rc::Rc;
use winit::window::Window;

pub struct Renderer {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
}

impl Renderer {
    pub fn new(window: Rc<Window>) -> anyhow::Result<Self> {
        let context =
            softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Renderer { window, surface })
    }

    pub fn draw(&mut self) {
        let size = self.window.inner_size(); // window size in pixels
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        self.surface.resize(w, h).unwrap(); // make the pixel buffer match the window
        let mut buffer = self.surface.buffer_mut().unwrap(); // get the pixel array
        buffer.fill(0); // set every pixel to black
        buffer.present().unwrap(); // show it on screen
    }
}
