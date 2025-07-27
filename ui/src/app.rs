
use freetype::face::LoadFlag;
use softbuffer::Surface;
use winit::{
    application::ApplicationHandler, dpi::PhysicalSize, event::{ElementState, WindowEvent}, event_loop::{ActiveEventLoop, OwnedDisplayHandle}, keyboard::{Key, NamedKey}, window::{Window, WindowAttributes, WindowId}
};

use crate::font::VIRTUE_TTF;

struct LateState {
    surf: Surface<OwnedDisplayHandle, Window>,
    font: freetype::Face<&'static [u8]>,
    threshold: u8
}

#[derive(Default)]
pub struct App {
    st: Option<LateState>
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_inner_size(PhysicalSize::new(1280, 960));
            //.with_resizable(false);

        let win = event_loop.create_window(attrs).unwrap();
        let ctx = softbuffer::Context::new(event_loop.owned_display_handle()).unwrap();
        let mut surf = Surface::new(&ctx, win).unwrap();

        let PhysicalSize { width, height } = surf.window().inner_size();
        surf.resize(
            width.try_into().unwrap(),
            height.try_into().unwrap()
        ).unwrap();

        let ft = freetype::Library::init().unwrap();
        let font = ft.new_memory_face2(VIRTUE_TTF, 0).unwrap();

        font.set_pixel_sizes(0, 12).unwrap();

        self.st = Some(LateState {
            surf, font,
            threshold: 0x80
        })
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(ref mut st) = self.st else { return };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            },
            WindowEvent::KeyboardInput { event, .. } => if event.state == ElementState::Pressed {
                match event.logical_key {
                    Key::Named(NamedKey::ArrowUp) => {
                        st.threshold = st.threshold.saturating_add(1);
                        st.surf.window().request_redraw();
                        println!("{:02X}", st.threshold);
                    },
                    Key::Named(NamedKey::ArrowDown) => {
                        st.threshold = st.threshold.saturating_sub(1);
                        st.surf.window().request_redraw();
                        println!("{:02X}", st.threshold);
                    },
                    _ => ()
                }
            },
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                st.surf.resize(
                    width.try_into().unwrap(),
                    height.try_into().unwrap()
                ).unwrap();
            },
            WindowEvent::RedrawRequested => {
                let PhysicalSize { width, height } = st.surf.window().inner_size();
                let (width, height) = (width as usize, height as usize);
                let mut buf = st.surf.buffer_mut().unwrap();

                buf.fill(0x899ce7);

                let mut pen_x = 20.;
                let pen_y = 20.;
                for c in "CREST, DRAGON, DRINK, Humanity,  Hybrid,".chars() {
                    st.font.load_char(c as usize,
                        LoadFlag::RENDER | LoadFlag::TARGET_MONO).unwrap();
                    
                    let slot = st.font.glyph();
                    let bmp = slot.bitmap();
                    let left = slot.bitmap_left() as usize;
                    let top = slot.bitmap_top() as usize;

                    for y in 0..bmp.rows() as usize {
                        for x in 0..bmp.width() as usize {
                            let pix = x + y * (bmp.pitch() as usize) * 8;
                            let idx = pix / 8;
                            let bit = pix % 8;
                            if bmp.buffer()[idx] & (0x80 >> bit) != 0 {
                                let pos_x = 4*(pen_x as usize + left + x);
                                let pos_y = 4*(pen_y as usize - top + y);
                                for xoff in 0..4 {
                                    for yoff in 0..4 {
                                        let xpos = pos_x + xoff;
                                        let ypos = pos_y + yoff;
                                        if xpos < width && ypos < height {
                                            buf[xpos + ypos * width] = 0;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    pen_x += (slot.advance().x as f32) / 64.;
                }

                buf.present().unwrap();
            },
            _ => ()
        }
    }
}