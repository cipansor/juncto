use crate::i18n::t;
use leptos::html;
use leptos::prelude::*;
use shared::DrawAction;
use wasm_bindgen::JsCast;

#[component]
pub fn Whiteboard(
    on_draw: Callback<DrawAction>,
    history: ReadSignal<Vec<DrawAction>>,
    reset: ReadSignal<u32>,
    append: ReadSignal<u32>,
    my_id: ReadSignal<Option<String>>,
    is_visitor: Signal<bool>,
) -> impl IntoView {
    let canvas_ref = NodeRef::<html::Canvas>::new();
    let (is_drawing, set_is_drawing) = signal(false);
    let (last_pos, set_last_pos) = signal(None::<(f64, f64)>);
    let (color, set_color) = signal("#000000".to_string());

    // Draw an action on the canvas
    let draw_on_canvas = move |action: &DrawAction| {
        if let Some(canvas) = canvas_ref.get() {
            let ctx = canvas
                .get_context("2d")
                .unwrap()
                .unwrap()
                .dyn_into::<web_sys::CanvasRenderingContext2d>()
                .unwrap();

            ctx.begin_path();
            ctx.set_line_width(action.width);
            #[allow(deprecated)]
            ctx.set_stroke_style(&wasm_bindgen::JsValue::from_str(&action.color));
            ctx.move_to(action.start_x, action.start_y);
            ctx.line_to(action.end_x, action.end_y);
            ctx.stroke();
        }
    };

    // Paint strokes on the canvas. Three paths share this effect:
    //  * first run (panel opened late): paint everything already retained.
    //  * `reset` bumped (join snapshot / throttled post-eviction rebuild): clear
    //    and repaint everything retained.
    //  * `append` bumped (a `Draw` arrived): paint just the appended tail.
    // A length-based diff cannot be used any more because evictions trim the
    // front of the history, so the length stays pinned at the cap.
    let paint_all = move || {
        if let Some(canvas) = canvas_ref.get() {
            let ctx = canvas
                .get_context("2d")
                .unwrap()
                .unwrap()
                .dyn_into::<web_sys::CanvasRenderingContext2d>()
                .unwrap();
            ctx.clear_rect(0.0, 0.0, canvas.width() as f64, canvas.height() as f64);
            for action in history.get_untracked() {
                draw_on_canvas(&action);
            }
        }
    };

    Effect::new(move |prev: Option<(u32, u32)>| {
        let reset = reset.get();
        let append = append.get();
        let actions = history.get();

        match prev {
            // Initial mount: history may already hold strokes (the panel can be
            // opened after drawing began), so paint it once.
            None => paint_all(),
            Some((last_reset, last_append)) => {
                if last_reset != reset {
                    paint_all();
                } else if append != last_append {
                    let delta = append.wrapping_sub(last_append) as usize;
                    let start = actions.len().saturating_sub(delta);
                    for i in start..actions.len() {
                        if let Some(action) = actions.get(i) {
                            // Filter local echo: we already painted it on mousemove.
                            if let Some(id) = my_id.get()
                                && action.sender_id == id
                            {
                                continue;
                            }
                            draw_on_canvas(action);
                        }
                    }
                }
            }
        }
        (reset, append)
    });

    let on_mousedown = move |ev: web_sys::MouseEvent| {
        if is_visitor.get_untracked() {
            return;
        }
        set_is_drawing.set(true);
        set_last_pos.set(Some((ev.offset_x() as f64, ev.offset_y() as f64)));
    };

    let on_mouseup = move |_| {
        set_is_drawing.set(false);
        set_last_pos.set(None);
    };

    let on_mousemove = move |ev: web_sys::MouseEvent| {
        if is_drawing.get()
            && let Some((start_x, start_y)) = last_pos.get()
        {
            let end_x = ev.offset_x() as f64;
            let end_y = ev.offset_y() as f64;

            let action = DrawAction {
                color: color.get(),
                width: 2.0,
                start_x,
                start_y,
                end_x,
                end_y,
                sender_id: my_id.get().unwrap_or_default(),
                // The server assigns the authoritative revision on broadcast.
                rev: 0,
            };

            // Draw locally immediately
            draw_on_canvas(&action);

            // Send to server
            on_draw.run(action);

            set_last_pos.set(Some((end_x, end_y)));
        }
    };

    view! {
        <div class="whiteboard-container" style="position: absolute; top: 0; left: 0; width: 100%; height: 100%; background: rgba(255, 255, 255, 0.9); z-index: 10; display: flex; flex-direction: column; align-items: center;">
            <div class="controls" style="margin: 10px; display: flex; gap: 10px; align-items: center;">
                <Show when=move || !is_visitor.get() fallback=|| view! { <span style="color: #666;">"Visitor Mode: Read-only"</span> }>
                    <label>{move || t("color")}</label>
                    <input
                        type="color"
                        prop:value=color
                        on:input=move |ev| set_color.set(event_target_value(&ev))
                        style="cursor: pointer;"
                    />
                </Show>
            </div>
            <canvas
                node_ref=canvas_ref
                width="800"
                height="600"
                style=move || format!("border: 1px solid black; cursor: {}; background: white;", if is_visitor.get() { "default" } else { "crosshair" })
                on:mousedown=on_mousedown
                on:mouseup=on_mouseup
                on:mouseleave=on_mouseup
                on:mousemove=on_mousemove
            />
        </div>
    }
}
#[cfg(test)]
mod tests {

    #[test]
    fn test_whiteboard_compiles() {
        // dummy test
        let _ = true;
    }
}
