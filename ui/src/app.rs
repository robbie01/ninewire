use std::{cell::Cell, iter, rc::Rc};

use leptos::{ev::MouseEvent, prelude::*};
use wasm_bindgen::prelude::*;
use web_sys::AddEventListenerOptions;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[component]
pub fn Window(z: ReadSignal<i32>, bring_me_up: impl Fn() + 'static) -> impl IntoView {
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

    view! {
        <div
            class="window"
            class:active=move || z.get() == 0
            class:collapsed=move || collapsed.get()
            style:z-index=move || format!("{}", z.get())
            style:left=move || format!("{}px", pos.get().0.max(0))
            style:top=move || format!("{}px", pos.get().1.max(0))
            on:mousedown=move |_| bring_me_up()
        >
            <div
                class="titlebar"
                on:mousedown=on_mouse_down
            >
                <button class="btn-close" on:mousedown=|e| e.set_cancel_bubble(true) />
                <div class="space-left" />
                <div class="title">Files</div>
                <div class="space-right" />
                <button class="btn-collapse" on:mousedown=move |e| {
                    e.set_cancel_bubble(true);
                    set_collapsed.update(|v| *v = !*v);
                } />
            </div>
            <div class="inner">aaa</div>
        </div>
    }
}

#[component]
pub fn App() -> impl IntoView {
    let n = 2;
    let zsr = Rc::new(Cell::new(iter::once(0).chain(iter::repeat(-1)).map(|v| RwSignal::new(v)).take(n).collect::<Vec<_>>()));

    let bring_me_up = {
        let zsr = zsr.clone();

        move |id: usize| {
            let zsr = zsr.clone();
            
            move || {
                let zs = zsr.take();
                let max_outside = zs.iter().enumerate().filter_map(|(i, v)| (i != id).then(|| v.get())).max().unwrap_or(0);
                for (i, &z) in zs.iter().enumerate() {
                    z.set(if i == id {
                        0
                    } else {
                        z.get() - max_outside - 1
                    })
                }
                zsr.set(zs);
            }
        }
    };

    let zs = zsr.take();

    let view = view! {
        <Window z=zs[0].read_only() bring_me_up=bring_me_up(0) />
        <Window z=zs[1].read_only() bring_me_up=bring_me_up(1) />
    };

    zsr.set(zs);

    view
}
