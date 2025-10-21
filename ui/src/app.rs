use std::{cell::Cell, rc::Rc};

use indexmap::IndexMap;
use js_sys::Function;
use leptos::{ev::MouseEvent, prelude::*};
use nohash::BuildNoHashHasher;
use sync_wrapper::SyncWrapper;
use wasm_bindgen::prelude::*;
use web_sys::AddEventListenerOptions;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    type Channel;

    #[wasm_bindgen(constructor)]
    fn new(onmessage: Function) -> Channel;
}

type Windows = IndexMap<usize, WindowData, BuildNoHashHasher<usize>>;

#[component]
fn Window(
    id: usize,
    windows: RwSignal<Windows>,
    size: ArcReadSignal<(i32, i32)>,
    #[prop(default = true)]
    use_inset: bool,
    children: AnyView
) -> impl IntoView {
    let (pos, set_pos) = signal((32, 32));
    let (collapsed, set_collapsed) = signal(false);
    
    let dragging = Rc::new(Cell::new(None));

    let on_mouse_move = Closure::<dyn Fn(MouseEvent)>::new({
        let dragging = dragging.clone();

        move |e: MouseEvent| {
            if let Some((last_x, last_y)) = dragging.get() {
                e.prevent_default();

                let x = e.screen_x();
                let y = e.screen_y();
                dragging.set(Some((x, y)));

                let dx = x - last_x;
                let dy = y - last_y;
                set_pos.update(|v| *v = (v.0 + dx, v.1 + dy))
            }
        }
    }).into_js_value();

    let on_mouse_up = Closure::<dyn Fn(MouseEvent)>::new({
        let dragging = dragging.clone();
        let on_mouse_move = on_mouse_move.clone();

        move |_| {
            document().remove_event_listener_with_callback("mousemove", on_mouse_move.unchecked_ref()).unwrap();
            dragging.set(None);
            set_pos.update(|v| *v = (v.0.max(0), v.1.max(0)));
        }
    }).into_js_value();

    let on_mouse_down = move |e: MouseEvent| {
        if e.button() == 0 {
            dragging.set(Some((e.screen_x(), e.screen_y())));
            let document = document();
            document.add_event_listener_with_callback("mousemove", on_mouse_move.unchecked_ref()).unwrap();
            let opts = AddEventListenerOptions::new();
            opts.set_once(true);
            document.add_event_listener_with_callback_and_add_event_listener_options("mouseup", on_mouse_up.unchecked_ref(), &opts).unwrap();
        }
    };

    let size2 = size.clone();

    view! {
        <div
            class="window"
            class:active=move || {
                let windows = windows.read();
                windows.last().is_some_and(|(&lid, _)| lid == id)
            }
            class:collapsed=move || collapsed.get()
            style:left=move || format!("{}px", pos.get().0.max(0))
            style:top=move || format!("{}px", pos.get().1.max(0))
            style=("--width", move || size.get().0.to_string())
            style=("--height", move || if collapsed.get() { String::new() } else { size2.get().1.to_string() })
            on:mousedown=move |_| {
                let mut windows = windows.write();
                let idx = windows.get_index_of(&id).unwrap();
                let len = windows.len();
                windows.move_index(idx, len - 1);
            }
        >
            <div
                class="titlebar"
                on:mousedown=on_mouse_down
                on:dblclick=move |_| set_collapsed.update(|v| *v = !*v)
            >
                <button class="btn-close"
                    on:mousedown=|e| e.set_cancel_bubble(true)
                    on:click=move |_| { windows.write().shift_remove(&id); }
                />
                <div class="space-left" />
                <div class="title">Files</div>
                <div class="space-right" />
                <button
                    class="btn-collapse"
                    on:mousedown=move |e| {
                        e.set_cancel_bubble(true);
                        set_collapsed.update(|v| *v = !*v);
                    }
                    on:dblclick=|e| e.set_cancel_bubble(true)
                />
            </div>
            <div class="inner" class:inset=use_inset>{children}</div>
        </div>
    }
}

struct WindowData {
    view: SyncWrapper<Option<AnyView>>,
    use_inset: bool,
    size: ArcRwSignal<(i32, i32)>
}

#[component]
pub fn App() -> impl IntoView {
    let windows = RwSignal::new(Windows::default());

    let window_id_ctr = Cell::new(0usize);
    let open_window = move |win: WindowData| {
        let id = window_id_ctr.get();
        window_id_ctr.set(id + 1);

        windows.write().insert(id, win)
    };

    open_window(WindowData {
        view: SyncWrapper::new(Some(view! { bbb }.into_any())),
        use_inset: false,
        size: ArcRwSignal::new((128, 96))
    });

    open_window(WindowData {
        view: SyncWrapper::new(Some(view! { aaa }.into_any())),
        use_inset: true,
        size: ArcRwSignal::new((128, 96))
    });

    // spawn_local(async move {
    //     loop {
    //         let p = Promise::new(&mut |res, _| set_timeout(move || { let _ = res.call0(&JsValue::null()); }, Duration::from_secs(2)));
    //         let _ = JsFuture::from(p).await;
    //         open_window(WindowData {
    //             view: Some(view! { "HWP" }.into_any()),
    //             use_inset: false,
    //             size: RwSignal::new((128, 128))
    //         });
    //     }
    // });

    view! {
        <For
            each=move || { windows.read().keys().copied().collect::<Vec<_>>() }
            key=|&id| id
            let(id)
        >{
            let win = &mut windows.write_untracked()[id];

            view! {
                <Window
                    id
                    size=win.size.read_only()
                    windows=windows
                    use_inset=win.use_inset
                    children=win.view.get_mut().take().unwrap()
                />
            }
        }</For>
    }
}
