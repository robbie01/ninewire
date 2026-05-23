
#![allow(clippy::missing_safety_doc)]

use async_task::Runnable;
use tokio::net::TcpStream;
use transport::PlainTcpTransport;
use winit::{application::ApplicationHandler, event_loop::{EventLoop, EventLoopProxy}};

use crate::{mpv::{IntoStreamInfo, StreamError}, nine::NineStream};

pub mod mpv;
mod nine;

#[derive(Debug)]
enum Event {
    MpvWakeup,
    CtrlC,
    Runnable(Runnable<()>)
}

struct App<'data, W, O> {
    proxy: &'data EventLoopProxy<Event>,
    wakeup: &'data W,
    open_nine: &'data O,
    mpv: Option<mpv::Handle<'static>>
}

impl<'data, W, O> App<'data, W, O> {
    fn spawn<'this>(&'this self, fut: impl Future<Output = ()> + 'this) {
        let proxy = self.proxy.clone();

        let (r, t) = unsafe { 
            async_task::spawn_unchecked(
                fut,
                move |r| {
                    let _ = proxy.send_event(Event::Runnable(r));
                }
            )
        };

        t.detach();
        r.schedule();
    }
}

impl<'data, W: Fn() + Sync, O: Fn(&str) -> Result<Box<dyn IntoStreamInfo>, StreamError> + Sync> ApplicationHandler<Event> for App<'data, W, O> {
    fn resumed(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        let mpv = mpv::Handle::new().unwrap();
        
        unsafe {
            mpv.set_wakeup_callback(self.wakeup);
        }
        mpv.register_protocol_ro("np", self.open_nine).unwrap();

        mpv.request_log_messages("info\0").unwrap();

        self.spawn(async {
            let mpv = self.mpv.as_ref().unwrap();
            let yes = mpv::NodeValue::Flag(true).into();

            mpv.set_property("force-window\0", &yes).await.unwrap();
            mpv.set_property("idle\0", &yes).await.unwrap();
            mpv.set_property("input-default-bindings\0", &yes).await.unwrap();
            mpv.set_property("input-vo-keyboard\0", &yes).await.unwrap();
            mpv.set_property("input-media-keys\0", &yes).await.unwrap();
            mpv.set_property("osc\0", &yes).await.unwrap();

            mpv.command(&["loadfile\0", "np://anime/The Melancholy of Haruhi Suzumiya (2006)/Season 1/[Blank] Suzumiya Haruhi no Yuuutsu 2006 - 02.mkv\0"]).await.unwrap()
        });

        self.mpv = Some(mpv);
    }

    fn window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: winit::event::WindowEvent,
    ) {
        println!("{_event:?}");
    }

    fn user_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, event: Event) {
        match event {
            Event::MpvWakeup => {
                while let Some(()) = self.mpv.as_mut().unwrap().poll_event(|ev| {
                    match ev {
                        mpv::Event::Shutdown => {
                            println!("shutdown received");
                            event_loop.exit()
                        },
                        mpv::Event::LogMessage { prefix: _, level: _, text } => {
                            println!("log: {}", text.to_str().unwrap().trim());
                        },
                        _ => println!("{ev:?}")
                    }
                }) {}
            },
            Event::CtrlC => {
                let fut = self.mpv.as_ref().unwrap().command(&["quit"]);
                self.spawn(async move {
                    fut.await.unwrap()
                });
            },
            Event::Runnable(r) => {
                r.run();
            }
        }
    }
}

fn main() {
    tracing_subscriber::fmt::init();

    let rt = tokio::runtime::Builder::new_multi_thread().enable_io().build().unwrap();

    let _guard = rt.enter();

    let root = rt.block_on(async {
        let tcp = TcpStream::connect("localhost:9998").await.unwrap();
        let transport = PlainTcpTransport::new(tcp).unwrap();
        let fs = client::Filesystem::new(transport).await.unwrap();
        println!("got the fs?");
        fs.attach("anonymous", "").await.unwrap()
    });
    println!("got the root");

    let open_nine = move |uri: &str| {
        let _guard = rt.enter();
        rt.block_on(root.open_at(uri.strip_prefix("np://").unwrap()))
            .map_err(|_| StreamError)
            .map(|f| Box::new(NineStream::new(f)) as Box<dyn IntoStreamInfo>)
    };

    let ev = EventLoop::<Event>::with_user_event().build().unwrap();
    
    let proxy = ev.create_proxy();
    ctrlc::set_handler(move || {
        let _ = proxy.send_event(Event::CtrlC);
    }).unwrap();

    let proxy = &ev.create_proxy();

    let wakeup = move || {
        let _ = proxy.send_event(Event::MpvWakeup);
    };

    ev.run_app(&mut App {
        proxy,
        wakeup: &wakeup,
        open_nine: &open_nine,
        mpv: None
    }).unwrap();
}