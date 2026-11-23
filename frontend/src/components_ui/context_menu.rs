use leptos::ev;
use leptos::prelude::*;

/// Right-click context menu opened on a video tile.
/// Shows participant-scoped actions: pin/unpin, volume fader, kick (host only).
#[component]
pub fn VideoContextMenu(
    open: ReadSignal<bool>,
    x: ReadSignal<i32>,
    y: ReadSignal<i32>,
    is_host: Signal<bool>,
    is_pinned: Signal<bool>,
    volume: Signal<f64>,
    on_pin: Callback<()>,
    on_kick: Callback<()>,
    on_volume: Callback<f64>,
    on_close: Callback<()>,
) -> impl IntoView {
    let close = move |_| on_close.run(());
    // Registered on `window`, so it outlives this component unless removed: the
    // handle is not detached on drop. `on_close` belongs to the room page, whose
    // signals are disposed when client-side navigation leaves it, and a stale
    // listener then panics on `get_untracked()`.
    let escape_listener = window_event_listener(ev::keydown, move |ev| {
        if ev.key() == "Escape" {
            on_close.run(())
        }
    });
    on_cleanup(move || escape_listener.remove());

    view! {
        <Show when=move || open.get()>
            <div class="overlay-backdrop" on:click=close></div>
            <div
                class="video-context-menu"
                style=move || format!("left: {}px; top: {}px;", x.get(), y.get())
            >
                <div class="context-menu-item" on:click=move |_| { on_pin.run(()); on_close.run(()); }>
                    {move || if is_pinned.get() { "Unpin participant" } else { "Pin participant" }}
                </div>
                <div class="context-menu-item" on:click=move |_| {} >
                    <label class="context-menu-label">"Volume"</label>
                    <input
                        type="range"
                        min="0"
                        max="1"
                        step="0.05"
                        value=move || volume.get()
                        class="context-menu-slider"
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                on_volume.run(v);
                            }
                        }
                    />
                </div>
                <Show when=move || is_host.get()>
                    <div class="context-menu-item danger" on:click=move |_| { on_kick.run(()); on_close.run(()); }>
                        "Kick participant"
                    </div>
                </Show>
            </div>
        </Show>
    }
}
