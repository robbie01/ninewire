use std::cell::Cell;

use indexmap::IndexSet;
use js_sys::Function;
use leptos::{ev::MouseEvent, prelude::*};
use nohash::{BuildNoHashHasher, IntMap};
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

type Windows = IndexSet<usize, BuildNoHashHasher<usize>>;

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
    
    let dragging = ArenaItem::new(None);

    let on_mouse_move = Closure::<dyn Fn(MouseEvent)>::new({
        move |e: MouseEvent| {
            if let Some((last_x, last_y)) = dragging.try_get_value().unwrap() {
                e.prevent_default();

                let x = e.screen_x();
                let y = e.screen_y();
                dragging.try_update_value(|v| *v = Some((x, y))).unwrap();

                let dx = x - last_x;
                let dy = y - last_y;
                set_pos.update(|v| *v = (v.0 + dx, v.1 + dy))
            }
        }
    }).into_js_value();

    let on_mouse_up = Closure::<dyn Fn(MouseEvent)>::new({
        let on_mouse_move = on_mouse_move.clone();

        move |e: MouseEvent| {
            if e.button() == 0 {
                document().remove_event_listener_with_callback("mousemove", on_mouse_move.unchecked_ref()).unwrap();
                dragging.try_update_value(|v| *v = None).unwrap();
                set_pos.update(|v| *v = (v.0.max(0), v.1.max(0)));
            }
        }
    }).into_js_value();

    let on_mouse_down = move |e: MouseEvent| {
        if e.button() == 0 {
            dragging.try_update_value(|v| *v = Some((e.screen_x(), e.screen_y()))).unwrap();
            let document = document();
            document.add_event_listener_with_callback("mousemove", on_mouse_move.unchecked_ref()).unwrap();
            let opts = AddEventListenerOptions::new();
            opts.set_once(true);
            document.add_event_listener_with_callback_and_add_event_listener_options("mouseup", on_mouse_up.unchecked_ref(), &opts).unwrap();
        }
    };

    view! {
        <div
            class="window"
            class:active=move || {
                let windows = windows.read();
                windows.last().is_some_and(|&lid| lid == id)
            }
            class:collapsed=move || collapsed.get()
            style:left=move || format!("{}px", pos.get().0.max(0))
            style:top=move || format!("{}px", pos.get().1.max(0))
            style=("--width", { let size = size.clone(); move || size.get().0.to_string() })
            style=("--height", move || if collapsed.get() { String::new() } else { size.get().1.to_string() })
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

struct WindowParams {
    view: SyncWrapper<AnyView>,
    use_inset: bool,
    size: ArcReadSignal<(i32, i32)>
}

#[component]
pub fn App() -> impl IntoView {
    let pending_windows = ArenaItem::new(IntMap::<usize, WindowParams>::default());
    let windows = RwSignal::new(Windows::default());

    let window_id_ctr = Cell::new(0usize);
    let open_window = move |win: WindowParams| {
        let id = window_id_ctr.get();
        window_id_ctr.set(id + 1);

        let fresh = pending_windows.try_update_value(|w| w.insert(id, win).is_none()).unwrap();
        assert!(fresh);
        let fresh = windows.write().insert(id);
        assert!(fresh);
    };

    open_window(WindowParams {
        view: SyncWrapper::new(view! { bbb }.into_any()),
        use_inset: false,
        size: ArcRwSignal::new((128, 96)).read_only()
    });

    open_window(WindowParams {
        view: SyncWrapper::new(view! { aaa }.into_any()),
        use_inset: true,
        size: ArcRwSignal::new((128, 96)).read_only()
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
            each=move || { windows.read().iter().copied().collect::<Vec<_>>() }
            key=|&id| id
            let(id)
        >{
            let win = pending_windows.try_update_value(|w| w.remove(&id)).flatten().unwrap();

            view! {
                <Window
                    id
                    size=win.size.clone()
                    windows=windows
                    use_inset=win.use_inset
                    children=win.view.into_inner()
                />
            }
        }</For>
    }
}
