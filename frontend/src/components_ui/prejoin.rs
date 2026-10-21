use crate::state::JoinOptions;
use browser_media::{
    AudioMonitor, DeviceInfo, get_audio_input_devices, get_user_media, get_video_input_devices,
};
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use web_sys::MediaStream;

#[component]
pub fn PrejoinScreen(
    on_join: Callback<JoinOptions>,
    is_connected: ReadSignal<bool>,
    #[prop(into, optional)] subject: Signal<Option<String>>,
    /// True when the server rejected a previous join because the room is
    /// locked with a password — shows the password input.
    #[prop(into, optional)]
    password_required: Signal<bool>,
) -> impl IntoView {
    let initial_settings = crate::storage::load_settings();
    let (display_name, set_display_name) = signal(
        initial_settings
            .display_name
            .clone()
            .unwrap_or_else(|| "Guest".to_string()),
    );
    let (avatar_url, set_avatar_url) = signal("".to_string());

    // Device Lists
    let (video_devices, set_video_devices) = signal(Vec::<DeviceInfo>::new());
    let (audio_devices, set_audio_devices) = signal(Vec::<DeviceInfo>::new());

    // Selected Devices
    let (selected_video_device, set_selected_video_device) = signal(initial_settings.camera_id);
    let (selected_audio_device, set_selected_audio_device) = signal(initial_settings.mic_id);

    // Toggles
    let (is_camera_on, set_is_camera_on) = signal(false);
    let (is_mic_on, set_is_mic_on) = signal(true);
    let (is_visitor, set_is_visitor) = signal(false);

    // Stream & Audio Monitor
    let (local_stream, set_local_stream) = signal(None::<MediaStream>);
    let (_audio_monitor, set_audio_monitor) = signal(None::<AudioMonitor>);
    let (is_speaking, set_is_speaking) = signal(false);

    // Load Devices on Mount
    Effect::new(move |_| {
        spawn_local(async move {
            let v_devices = get_video_input_devices().await.ok().unwrap_or_default();
            let a_devices = get_audio_input_devices().await.ok().unwrap_or_default();

            batch(move || {
                set_video_devices.set(v_devices.clone());
                let saved_vid = selected_video_device.get_untracked();
                let vid_valid = saved_vid
                    .as_ref()
                    .is_some_and(|id| v_devices.iter().any(|d| &d.device_id == id));
                if !vid_valid && let Some(first) = v_devices.first() {
                    set_selected_video_device.set(Some(first.device_id.clone()));
                }

                set_audio_devices.set(a_devices.clone());
                let saved_aid = selected_audio_device.get_untracked();
                let aid_valid = saved_aid
                    .as_ref()
                    .is_some_and(|id| a_devices.iter().any(|d| &d.device_id == id));
                if !aid_valid && let Some(first) = a_devices.first() {
                    set_selected_audio_device.set(Some(first.device_id.clone()));
                }
            });
        });
    });

    // Stop stream helper
    let stop_stream = move || {
        if let Some(stream) = local_stream.get_untracked() {
            let tracks = stream.get_tracks();
            for i in 0..tracks.length() {
                if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                    track.stop();
                }
            }
        }
        set_local_stream.set(None);
        set_audio_monitor.set(None);
    };

    // Update Stream when settings change
    Effect::new(move |_| {
        let cam_on = is_camera_on.get();
        let mic_on = is_mic_on.get();
        let v_id = selected_video_device.get();
        let a_id = selected_audio_device.get();

        spawn_local(async move {
            if let Some(stream) = local_stream.get_untracked() {
                let tracks = stream.get_tracks();
                for i in 0..tracks.length() {
                    if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                        track.stop();
                    }
                }
            }
            set_local_stream.set(None);
            set_audio_monitor.set(None);

            if (cam_on || mic_on)
                && let Ok(stream) = get_user_media(cam_on, true, v_id, a_id, Some("hd")).await
            {
                let audio_tracks = stream.get_audio_tracks();
                for i in 0..audio_tracks.length() {
                    if let Ok(track) = audio_tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                        track.set_enabled(mic_on);
                    }
                }

                set_local_stream.set(Some(stream.clone()));

                if mic_on {
                    let on_speaking = Box::new(move |speaking: bool| {
                        set_is_speaking.set(speaking);
                    });
                    if let Ok(monitor) = AudioMonitor::new(&stream, on_speaking, None, None, false)
                    {
                        set_audio_monitor.set(Some(monitor));
                    }
                }
            }
        });
    });

    on_cleanup(move || {
        stop_stream();
    });

    let (room_password, set_room_password) = signal(String::new());

    let handle_join = move |_| {
        stop_stream();
        let av = avatar_url.get_untracked();
        let pw = room_password.get_untracked();
        on_join.run(JoinOptions {
            display_name: display_name.get_untracked(),
            mic_enabled: is_mic_on.get_untracked(),
            camera_enabled: is_camera_on.get_untracked(),
            audio_device_id: selected_audio_device.get_untracked(),
            video_device_id: selected_video_device.get_untracked(),
            is_visitor: is_visitor.get_untracked(),
            avatar_url: if av.is_empty() { None } else { Some(av) },
            password: if pw.is_empty() { None } else { Some(pw) },
        });
    };

    let video_ref = NodeRef::<html::Video>::new();
    Effect::new(move |_| {
        if let Some(video) = video_ref.get() {
            if let Some(stream) = local_stream.get() {
                video.set_src_object(Some(&stream));
            } else {
                video.set_src_object(None);
            }
        }
    });

    view! {
        <div class="prejoin-container">
            <div class="prejoin-card">
                <h2>"Join Meeting"</h2>
                <Show when=move || subject.get().as_ref().is_some_and(|s| !s.is_empty())>
                    <div
                        id="prejoin-subject"
                        class="badge-info"
                        style="margin-bottom: 20px; text-align: center; justify-content: center; width: 100%;"
                    >
                        {move || subject.get().unwrap_or_default()}
                    </div>
                </Show>

                // Video Preview Box
                <div style="position: relative; width: 100%; height: 240px; background: #090d16; margin-bottom: 20px; border-radius: var(--radius-lg); overflow: hidden; display: flex; align-items: center; justify-content: center; border: 1px solid var(--border-color); box-shadow: var(--shadow-md);">
                    <Show when=move || is_camera_on.get() fallback=|| view! {
                        <div class="camera-off-text" style="color: var(--text-muted); font-size: var(--font-size-sm); display: flex; flex-direction: column; align-items: center; gap: 8px;">
                            <span style="font-size: 2rem;">"📷"</span>
                            <span>"Camera is Off"</span>
                        </div>
                    }>
                        <video
                            node_ref=video_ref
                            autoplay
                            muted
                            playsinline
                            style="width: 100%; height: 100%; object-fit: cover;"
                        />
                    </Show>

                    // Audio Meter / Status Badge
                    <Show when=move || is_speaking.get() && is_mic_on.get()>
                        <div style="position: absolute; bottom: 12px; right: 12px; width: 16px; height: 16px; background: var(--success-color); border-radius: 50%; border: 2px solid white; box-shadow: 0 0 10px var(--success-glow);"></div>
                    </Show>
                    <Show when=move || !is_mic_on.get()>
                        <div style="position: absolute; bottom: 12px; right: 12px; color: #f87171; background: rgba(15, 23, 42, 0.8); backdrop-filter: var(--glass-backdrop); padding: 4px 10px; border-radius: var(--radius-sm); font-size: var(--font-size-xs); border: 1px solid var(--border-color);">
                            "🔇 Muted"
                        </div>
                    </Show>
                </div>

                // Media Toggle Controls
                <div style="display: flex; gap: 16px; justify-content: center; margin-bottom: 24px;">
                    <button
                        on:click=move |_| set_is_camera_on.update(|v| *v = !*v)
                        class=move || format!("toolbox-btn {}", if is_camera_on.get() { "active" } else { "danger" })
                        title="Toggle Camera"
                    >
                         {move || if is_camera_on.get() { "📷" } else { "🚫" }}
                    </button>
                    <button
                        on:click=move |_| set_is_mic_on.update(|v| *v = !*v)
                        class=move || format!("toolbox-btn {}", if is_mic_on.get() { "active" } else { "danger" })
                        title="Toggle Microphone"
                    >
                         {move || if is_mic_on.get() { "🎤" } else { "🔇" }}
                    </button>
                </div>

                // Device Selectors
                <div class="input-group">
                    <label class="input-label">"Camera Device"</label>
                    <select
                        class="styled-select"
                        on:change=move |ev| set_selected_video_device.set(Some(event_target_value(&ev)))
                        prop:value=move || selected_video_device.get().unwrap_or_default()
                        disabled=move || !is_camera_on.get()
                    >
                        <For
                            each=move || video_devices.get()
                            key=|d| d.device_id.clone()
                            children=move |device| {
                                view! {
                                    <option value=device.device_id>{device.label}</option>
                                }
                            }
                        />
                    </select>
                </div>

                <div class="input-group">
                    <label class="input-label">"Microphone Device"</label>
                    <select
                        class="styled-select"
                        on:change=move |ev| set_selected_audio_device.set(Some(event_target_value(&ev)))
                        prop:value=move || selected_audio_device.get().unwrap_or_default()
                    >
                        <For
                            each=move || audio_devices.get()
                            key=|d| d.device_id.clone()
                            children=move |device| {
                                view! {
                                    <option value=device.device_id>{device.label}</option>
                                }
                            }
                        />
                    </select>
                </div>

                // Name Input & Avatar
                <div class="input-group">
                    <label class="input-label" for="display-name">"Display Name"</label>
                    <input
                        type="text"
                        id="display-name"
                        class="styled-input"
                        maxlength="64"
                        on:input=move |ev| set_display_name.set(event_target_value(&ev))
                        prop:value=move || display_name.get()
                        placeholder="Enter your name"
                    />
                </div>

                <div class="input-group">
                    <label class="input-label" for="avatar-url">"Avatar URL (Optional)"</label>
                    <input
                        type="url"
                        id="avatar-url"
                        class="styled-input"
                        maxlength="2048"
                        on:input=move |ev| set_avatar_url.set(event_target_value(&ev))
                        prop:value=move || avatar_url.get()
                        placeholder="https://example.com/avatar.png"
                    />
                </div>

                <div style="margin-bottom: 20px; display: flex; align-items: center; gap: 10px; font-size: var(--font-size-sm); color: var(--text-secondary); text-align: left;">
                    <input
                        type="checkbox"
                        id="visitor-mode"
                        style="accent-color: var(--primary-color); width: 16px; height: 16px; cursor: pointer;"
                        prop:checked=is_visitor
                        on:change=move |ev| set_is_visitor.set(event_target_checked(&ev))
                    />
                    <label for="visitor-mode" style="cursor: pointer;">"Join as Visitor (Read-only)"</label>
                </div>

                <Show when=move || password_required.get()>
                    <div class="input-group">
                        <label class="input-label" for="room-password">"Room Password"</label>
                        <input
                            type="password"
                            id="room-password"
                            class="styled-input"
                            on:input=move |ev| set_room_password.set(event_target_value(&ev))
                            prop:value=move || room_password.get()
                            placeholder="Enter room password"
                        />
                    </div>
                </Show>

                <button
                    class="join-btn btn btn-success"
                    on:click=handle_join
                    disabled=move || !is_connected.get()
                    style="width: 100%; padding: 14px; font-size: 1rem; font-weight: 600; margin-top: 8px;"
                >
                    {move || if is_connected.get() { "Join Meeting" } else { "Connecting..." }}
                </button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_prejoin_compiles() {
        let owner = Owner::new();
        owner.set();
        let on_join = Callback::new(|_: JoinOptions| {});
        let (is_connected, _) = signal(true);

        let _view = view! {
            <PrejoinScreen on_join=on_join is_connected=is_connected />
        };
        let _ = true;
    }
}
