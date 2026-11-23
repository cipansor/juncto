use crate::utils::create_room_url;
use leptos::prelude::*;
use leptos_router::hooks::*;

#[component]
pub fn Home() -> impl IntoView {
    let (room_name, set_room_name) = signal("My Meeting".to_string());
    let navigate = use_navigate();

    let settings = crate::storage::load_settings();
    let (recent_rooms, _set_recent_rooms) = signal(settings.recent_rooms);

    let (name_error, set_name_error) = signal(false);
    let nav_create = navigate.clone();
    let create_meeting = move |_| {
        let name = room_name.get();
        // A whitespace-only name produces the route /room/%20%20%20, which
        // silently creates a room nobody can find again.
        if name.trim().is_empty() {
            set_name_error.set(true);
            return;
        }
        set_name_error.set(false);
        let url = create_room_url(name.trim());
        nav_create(&url, Default::default());
    };

    view! {
        <div class="welcome-container">
            <div class="hero-card">
                <h1>"Welcome to Juncto (Rust Edition)"</h1>
                <p>"High-Performance WebRTC Video Conferencing in Rust"</p>

                <div class="input-group">
                    <label class="input-label" for="meeting-name">"Room Name"</label>
                    <input
                        type="text"
                        id="meeting-name"
                        class="styled-input"
                        on:input=move |ev| {
                            set_room_name.set(event_target_value(&ev));
                            set_name_error.set(false);
                        }
                        prop:value=room_name
                        placeholder="Enter meeting name..."
                    />
                    <Show when=move || name_error.get()>
                        <p id="meeting-name-error" class="form-error">
                            "Enter a room name to start a meeting."
                        </p>
                    </Show>
                </div>

                <button
                    on:click=create_meeting
                    class="create-btn btn btn-primary"
                    style="width: 100%; padding: 12px; font-size: 1rem; font-weight: 600;"
                >
                    "🚀 Start Meeting"
                </button>

                <Show when=move || !recent_rooms.get().is_empty()>
                    <div style="margin-top: 32px; border-top: 1px solid var(--border-color); padding-top: 24px; text-align: left;">
                        <span class="input-label">"Recent Meetings"</span>
                        <div style="display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px;">
                            <For
                                each=move || recent_rooms.get()
                                key=|r| r.clone()
                                children={
                                    let nav_loop = navigate.clone();
                                    move |r| {
                                        let r_clone = r.clone();
                                        let nav = nav_loop.clone();
                                        view! {
                                            <button
                                                class="btn btn-outline"
                                                style="border-radius: var(--radius-full); padding: 6px 14px; font-size: var(--font-size-xs);"
                                                on:click=move |_| {
                                                    let url = format!("/room/{}", urlencoding::encode(&r_clone));
                                                    nav(&url, Default::default());
                                                }
                                            >
                                                "📌 " {r}
                                            </button>
                                        }
                                    }
                                }
                            />
                        </div>
                    </div>
                </Show>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_home_compiles() {
        let _ = true;
    }
}
