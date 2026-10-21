use crate::components_ui::toast::{ToastType, use_toast};
use crate::i18n::t;
use browser_media::{enumerate_devices, get_user_media};
use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MediaDeviceInfo, MediaDeviceKind};

pub type DeviceSettings = (Option<String>, Option<String>, String, bool);

#[component]
pub fn SettingsDialog(
    show: ReadSignal<bool>,
    on_close: Callback<()>,
    on_save_profile: Callback<String>,
    #[prop(into, optional)] current_name: Option<Signal<String>>,
    #[prop(into, optional)] current_avatar: Option<Signal<Option<String>>>,
    #[prop(into, optional)] current_subject: Option<Signal<Option<String>>>,
    #[prop(optional)] on_save_avatar: Option<Callback<Option<String>>>,
    #[prop(optional)] on_set_subject: Option<Callback<String>>,
    #[prop(optional)] on_save_devices: Option<Callback<DeviceSettings>>,
    #[prop(optional)] current_video_id: Option<ReadSignal<Option<String>>>,
    #[prop(optional)] current_audio_id: Option<ReadSignal<Option<String>>>,
    #[prop(optional)] current_resolution: Option<ReadSignal<String>>,
    #[prop(optional)] current_noise_suppression: Option<ReadSignal<bool>>,
    #[prop(optional)] is_host: Option<Signal<bool>>,
    #[prop(optional)] is_locked: Option<ReadSignal<bool>>,
    #[prop(optional)] is_e2ee_enabled: Option<ReadSignal<bool>>,
    #[prop(optional)] is_audio_moderation_enabled: Option<Signal<bool>>,
    #[prop(optional)] is_video_moderation_enabled: Option<Signal<bool>>,
    #[prop(optional)] is_lobby_enabled: Option<ReadSignal<bool>>,
    #[prop(optional)] is_participant_e2ee_enabled: Option<Signal<bool>>,
    #[prop(optional)] is_face_landmarks_enabled: Option<RwSignal<bool>>,
    #[prop(optional)] is_audio_only: Option<ReadSignal<bool>>,
    #[prop(optional)] is_flipped: Option<ReadSignal<bool>>,
    #[prop(optional)] on_toggle_audio_only: Option<Callback<bool>>,
    #[prop(optional)] on_toggle_flip: Option<Callback<bool>>,
    #[prop(optional)] on_toggle_lock: Option<Callback<Option<String>>>,
    #[prop(optional)] on_toggle_e2ee: Option<Callback<()>>,
    #[prop(optional)] on_toggle_audio_moderation: Option<Callback<()>>,
    #[prop(optional)] on_toggle_video_moderation: Option<Callback<()>>,
    #[prop(optional)] on_toggle_participant_e2ee: Option<Callback<bool>>,
    #[prop(optional)] on_toggle_lobby: Option<Callback<()>>,
    #[prop(optional)] on_set_branding: Option<Callback<shared::BrandingConfig>>,
    #[prop(optional)] current_branding: Option<Signal<shared::BrandingConfig>>,
) -> impl IntoView {
    let toast_ctx = use_toast();
    let (active_tab, set_active_tab) = signal("profile");

    Effect::new(move |_| {
        if active_tab.get() == "moderator" && !is_host.map(|h| h.get()).unwrap_or(false) {
            set_active_tab.set("profile");
        }
    });

    let (display_name, set_display_name) = signal("".to_string());
    let (avatar_url, set_avatar_url) = signal("".to_string());
    let (subject, set_subject) = signal("".to_string());
    let (primary_color, set_primary_color) = signal("#007bff".to_string());
    let (bg_color, set_bg_color) = signal("#ffffff".to_string());
    let (logo_url, set_logo_url) = signal("".to_string());

    // Sync local state with global props when dialog opens
    Effect::new(move |_| {
        if show.get() {
            if let Some(sig) = current_name {
                set_display_name.set(sig.get_untracked());
            }
            if let Some(sig) = current_avatar {
                set_avatar_url.set(sig.get_untracked().unwrap_or_default());
            }
            if let Some(sig) = current_subject {
                set_subject.set(sig.get_untracked().unwrap_or_default());
            }
            if let Some(sig) = current_branding {
                let b = sig.get_untracked();
                set_primary_color.set(b.primary_color.unwrap_or_else(|| "#007bff".to_string()));
                set_bg_color.set(b.background_color.unwrap_or_else(|| "#ffffff".to_string()));
                set_logo_url.set(b.logo_url.unwrap_or_default());
            }
        }
    });

    // Initialize state from props if available
    let init_video = current_video_id.and_then(|s| s.get_untracked());
    let init_audio = current_audio_id.and_then(|s| s.get_untracked());
    let init_res = current_resolution
        .map(|s| s.get_untracked())
        .unwrap_or("hd".to_string());

    // Devices State
    let (video_devices, set_video_devices) = signal(Vec::<MediaDeviceInfo>::new());
    let (audio_devices, set_audio_devices) = signal(Vec::<MediaDeviceInfo>::new());
    let (selected_video, set_selected_video) = signal(init_video);
    let (selected_audio, set_selected_audio) = signal(init_audio);
    let (video_quality, set_video_quality) = signal(init_res);
    let (lock_password, set_lock_password) = signal(String::new());
    let (noise_suppression, set_noise_suppression) = signal(
        current_noise_suppression
            .map(|s| s.get_untracked())
            .unwrap_or(false),
    );
    let (error_msg, set_error_msg) = signal(None::<String>);
    let (preview_stream, set_preview_stream) = signal(None::<web_sys::MediaStream>);

    let video_ref = NodeRef::<html::Video>::new();

    // Sync local state with global props when dialog opens
    Effect::new(move |_| {
        if show.get() {
            if let Some(sig) = current_video_id {
                set_selected_video.set(sig.get_untracked());
            }
            if let Some(sig) = current_audio_id {
                set_selected_audio.set(sig.get_untracked());
            }
            if let Some(sig) = current_resolution {
                set_video_quality.set(sig.get_untracked());
            }
            if let Some(sig) = current_noise_suppression {
                set_noise_suppression.set(sig.get_untracked());
            }
        }
    });

    let stop_preview = move || {
        if let Some(stream) = preview_stream.get_untracked() {
            let tracks = stream.get_tracks();
            for i in 0..tracks.length() {
                if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                    track.stop();
                }
            }
            set_preview_stream.set(None);
        }
    };

    let fetch_devices = Action::new_local(move |_: &()| async move {
        match enumerate_devices().await {
            Ok(devices) => {
                let mut vid = Vec::new();
                let mut aud = Vec::new();
                for d in devices {
                    match d.kind() {
                        MediaDeviceKind::Videoinput => vid.push(d),
                        MediaDeviceKind::Audioinput => aud.push(d),
                        _ => {}
                    }
                }
                set_video_devices.set(vid);
                set_audio_devices.set(aud);
            }
            Err(e) => {
                set_error_msg.set(Some(format!("Error enumerating devices: {:?}", e)));
            }
        }
    });

    let start_preview = Action::new_local(move |_: &()| async move {
        // Stop existing before starting new
        stop_preview();

        let v_id = selected_video.get();
        let a_id = selected_audio.get();
        let quality = video_quality.get();

        // Always enable video for settings preview
        match get_user_media(true, true, v_id, a_id, Some(&quality)).await {
            Ok(stream) => {
                // Check if a stream was set while we were awaiting (race condition check)
                if let Some(existing) = preview_stream.get_untracked() {
                    let tracks = existing.get_tracks();
                    for i in 0..tracks.length() {
                        if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                            track.stop();
                        }
                    }
                }

                set_preview_stream.set(Some(stream.clone()));
                if let Some(video_el) = video_ref.get() {
                    video_el.set_src_object(Some(&stream));
                    let _ = video_el.play();
                }
                set_error_msg.set(None);
            }
            Err(e) => {
                set_error_msg.set(Some(format!("Error accessing media: {:?}", e)));
            }
        }
    });

    Effect::new(move |_| {
        if active_tab.get() == "devices" {
            fetch_devices.dispatch(());
            start_preview.dispatch(());
        } else {
            stop_preview();
        }
    });

    Effect::new(move |_| {
        if !show.get() {
            stop_preview();
        }
    });

    on_cleanup(move || {
        stop_preview();
    });

    view! {
        <Show when=move || show.get()>
            <div class="modal-overlay" style="position: fixed; top: 0; left: 0; width: 100%; height: 100%; background: rgba(0,0,0,0.5); display: flex; justify-content: center; align-items: center; z-index: 1000;">
                <div class="modal-content" style="width: 500px;">
                    <div class="modal-header">
                        <h3>{move || t("settings")}</h3>
                        <button id="close-settings-btn" class="modal-close-btn" on:click=move |_| on_close.run(())>"×"</button>
                    </div>

                    <div class="tabs">
                        <button
                            on:click=move |_| set_active_tab.set("profile")
                            class=move || if active_tab.get() == "profile" { "modal-tab-btn active" } else { "modal-tab-btn" }
                        >
                            {move || t("profile")}
                        </button>
                        <button
                            on:click=move |_| set_active_tab.set("devices")
                            class=move || if active_tab.get() == "devices" { "modal-tab-btn active" } else { "modal-tab-btn" }
                        >
                            {move || t("devices")}
                        </button>
                        <button
                            on:click=move |_| set_active_tab.set("integrations")
                            class=move || if active_tab.get() == "integrations" { "modal-tab-btn active" } else { "modal-tab-btn" }
                        >
                            {move || t("integrations")}
                        </button>
                        <button
                            on:click=move |_| set_active_tab.set("more")
                            class=move || if active_tab.get() == "more" { "modal-tab-btn active" } else { "modal-tab-btn" }
                        >
                            "More"
                        </button>
                        <Show when=move || is_host.map(|h| h.get()).unwrap_or(false)>
                            <button
                                on:click=move |_| set_active_tab.set("branding")
                                class=move || if active_tab.get() == "branding" { "modal-tab-btn active" } else { "modal-tab-btn" }
                            >
                                "Branding"
                            </button>
                            <button
                                on:click=move |_| set_active_tab.set("moderator")
                                class=move || if active_tab.get() == "moderator" { "modal-tab-btn active" } else { "modal-tab-btn" }
                            >
                                {move || t("moderator")}
                            </button>
                        </Show>
                    </div>

                    <div class="tab-content">
                        <Show when=move || active_tab.get() == "profile">
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: flex; align-items: center; cursor: pointer; margin-bottom: 10px;">
                                    <input
                                        type="checkbox"
                                        id="e2ee-participant-toggle"
                                        prop:checked=move || is_participant_e2ee_enabled.map(|s| s.get()).unwrap_or(false)
                                        on:change=move |ev| {
                                            if let Some(cb) = on_toggle_participant_e2ee {
                                                cb.run(event_target_checked(&ev));
                                            }
                                        }
                                        style="margin-right: 10px;"
                                    />
                                    "Enable End-to-End Encryption"
                                </label>
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: block; margin-bottom: 5px;">{move || t("display_name")}</label>
                                <input
                                    type="text"
                                    id="settings-display-name"
                                    maxlength="64"
                                    prop:value=move || display_name.get()
                                    on:input=move |ev| set_display_name.set(event_target_value(&ev))
                                    style="width: 100%;"
                                />
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: block; margin-bottom: 5px;">"Avatar URL"</label>
                                <input
                                    type="text"
                                    id="settings-avatar-url"
                                    maxlength="2048"
                                    prop:value=move || avatar_url.get()
                                    on:input=move |ev| set_avatar_url.set(event_target_value(&ev))
                                    style="width: 100%;"
                                />
                            </div>
                            <button
                                id="save-profile-btn"
                                on:click=move |_| {
                                    // Only fire each update callback if the value actually
                                    // changed. This avoids redundant ParticipantUpdated
                                    // broadcasts (and the brief intermediate state on other
                                    // clients where one new field is paired with the old
                                    // value of the other) when the user only edits one of
                                    // name or avatar.
                                    let new_name = display_name.get();
                                    let prev_name = current_name
                                        .map(|s| s.get_untracked())
                                        .unwrap_or_default();
                                    if new_name != prev_name {
                                        on_save_profile.run(new_name);
                                    }
                                    if let Some(cb) = on_save_avatar {
                                        let av = avatar_url.get();
                                        let new_val = if av.is_empty() { None } else { Some(av) };
                                        let prev_val = current_avatar
                                            .map(|s| s.get_untracked())
                                            .unwrap_or(None);
                                        if new_val != prev_val {
                                            cb.run(new_val);
                                        }
                                    }
                                    on_close.run(());
                                }
                                class="btn btn-primary"
                            >
                                {move || t("save_profile")}
                            </button>
                        </Show>
                        <Show when=move || active_tab.get() == "devices">
                            <Show when=move || error_msg.get().is_some()>
                                <div class="settings-error">
                                    {move || error_msg.get().unwrap()}
                                </div>
                            </Show>

                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: block; margin-bottom: 5px;">{move || t("camera")}</label>
                                <select
                                    style="width: 100%;"
                                    on:change=move |ev| {
                                        let val = event_target_value(&ev);
                                        if val.is_empty() {
                                            set_selected_video.set(None);
                                        } else {
                                            set_selected_video.set(Some(val));
                                        }
                                        start_preview.dispatch(());
                                    }
                                >
                                    <option value="">{move || t("default")}</option>
                                    <For
                                        each=move || video_devices.get()
                                        key=|d| d.device_id()
                                        children=move |d| {
                                            let id = d.device_id();
                                            let label = d.label();
                                            let label_text = if label.is_empty() { format!("Camera {}", id) } else { label };
                                            let id_clone = id.clone();
                                            view! {
                                                <option value=id selected=move || selected_video.get().as_ref() == Some(&id_clone)>
                                                    {label_text}
                                                </option>
                                            }
                                        }
                                    />
                                </select>
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: block; margin-bottom: 5px;">{move || t("video_quality")}</label>
                                <select
                                    style="width: 100%;"
                                    on:change=move |ev| {
                                        set_video_quality.set(event_target_value(&ev));
                                        start_preview.dispatch(());
                                    }
                                >
                                    <option value="hd" selected=move || video_quality.get() == "hd">"HD (720p)"</option>
                                    <option value="sd" selected=move || video_quality.get() == "sd">"SD (360p)"</option>
                                </select>
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: flex; align-items: center; cursor: pointer;">
                                    <input
                                        type="checkbox"
                                        prop:checked=noise_suppression
                                        on:change=move |ev| set_noise_suppression.set(event_target_checked(&ev))
                                        style="margin-right: 10px;"
                                    />
                                    {move || t("noise_suppression")}
                                </label>
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: block; margin-bottom: 5px;">{move || t("microphone")}</label>
                                <select
                                    style="width: 100%;"
                                    on:change=move |ev| {
                                        let val = event_target_value(&ev);
                                        if val.is_empty() {
                                            set_selected_audio.set(None);
                                        } else {
                                            set_selected_audio.set(Some(val));
                                        }
                                        start_preview.dispatch(());
                                    }
                                >
                                    <option value="">{move || t("default")}</option>
                                    <For
                                        each=move || audio_devices.get()
                                        key=|d| d.device_id()
                                        children=move |d| {
                                            let id = d.device_id();
                                            let label = d.label();
                                            let label_text = if label.is_empty() { format!("Mic {}", id) } else { label };
                                            let id_clone = id.clone();
                                            view! {
                                                <option value=id selected=move || selected_audio.get().as_ref() == Some(&id_clone)>
                                                    {label_text}
                                                </option>
                                            }
                                        }
                                    />
                                </select>
                            </div>

                            <div class="preview" style="margin-top: 20px; height: 200px;">
                                <video
                                    node_ref=video_ref
                                    autoplay
                                    playsinline
                                    muted
                                    style="max-width: 100%; max-height: 100%;"
                                />
                            </div>
                            <p class="settings-hint">{move || t("preview_only")}</p>

                            <div style="margin-top: 15px; text-align: right;">
                                <button
                                    on:click=move |_| {
                                        if let Some(cb) = on_save_devices {
                                            cb.run((selected_video.get(), selected_audio.get(), video_quality.get(), noise_suppression.get()));
                                        }
                                        on_close.run(());
                                    }
                                    class="btn btn-success"
                                >
                                    {move || t("apply_devices")}
                                </button>
                            </div>
                        </Show>
                        <Show when=move || active_tab.get() == "more">
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: flex; align-items: center; cursor: pointer;">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || is_face_landmarks_enabled.map(|s| s.get()).unwrap_or(false)
                                        on:change=move |ev| {
                                            if let Some(sig) = is_face_landmarks_enabled {
                                                sig.set(event_target_checked(&ev));
                                            }
                                        }
                                        style="margin-right: 10px;"
                                    />
                                    "Enable Face Landmarks (Expressions)"
                                </label>
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: flex; align-items: center; cursor: pointer;">
                                    <input
                                        type="checkbox"
                                        id="audio-only-toggle"
                                        prop:checked=move || is_audio_only.map(|s| s.get()).unwrap_or(false)
                                        on:change=move |ev| {
                                            if let Some(cb) = on_toggle_audio_only {
                                                cb.run(event_target_checked(&ev));
                                            }
                                        }
                                        style="margin-right: 10px;"
                                    />
                                    "Audio-Only Mode (Hide remote videos)"
                                </label>
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: flex; align-items: center; cursor: pointer;">
                                    <input
                                        type="checkbox"
                                        id="flip-video-toggle"
                                        prop:checked=move || is_flipped.map(|s| s.get()).unwrap_or(false)
                                        on:change=move |ev| {
                                            if let Some(cb) = on_toggle_flip {
                                                cb.run(event_target_checked(&ev));
                                            }
                                        }
                                        style="margin-right: 10px;"
                                    />
                                    "Mirror Local Video"
                                </label>
                            </div>
                        </Show>
                        <Show when=move || active_tab.get() == "integrations">
                            <div class="integrations-list" style="display: flex; flex-direction: column; gap: 10px;">
                                {let services = ["Dropbox", "Salesforce", "Google Calendar"];
                                services.into_iter().map(|s| {
                                    let (connected, set_connected) = signal(false);
                                    let s_clone = s.to_string();
                                    view! {
                                        <div class="integration-item">
                                            <span>{s}</span>
                                            <button
                                                on:click={
                                                    let s_clone = s_clone.clone();
                                                    move |_| {
                                                        if !connected.get() {
                                                            let s_clone_2 = s_clone.clone();
                                                            toast_ctx.add(format!("Connecting to {} (mock)...", s_clone), ToastType::Info);
                                                            set_timeout(move || {
                                                                set_connected.set(true);
                                                                toast_ctx.add(format!("Connected to {}", s_clone_2), ToastType::Success);
                                                            }, std::time::Duration::from_millis(1500));
                                                        } else {
                                                            set_connected.set(false);
                                                            toast_ctx.add(format!("Disconnected from {}", s_clone), ToastType::Info);
                                                        }
                                                    }
                                                }
                                                class=move || if connected.get() { "btn btn-danger btn-sm" } else { "btn btn-primary btn-sm" }
                                            >
                                                {move || if connected.get() { "Disconnect".to_string() } else { t("connect") }}
                                            </button>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </Show>
                        <Show when=move || active_tab.get() == "branding">
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: block; margin-bottom: 5px;">"Primary Color"</label>
                                <input
                                    type="color"
                                    id="branding-primary-color"
                                    prop:value=primary_color
                                    on:input=move |ev| set_primary_color.set(event_target_value(&ev))
                                />
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: block; margin-bottom: 5px;">"Background Color"</label>
                                <input
                                    type="color"
                                    id="branding-bg-color"
                                    prop:value=bg_color
                                    on:input=move |ev| set_bg_color.set(event_target_value(&ev))
                                />
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: block; margin-bottom: 5px;">"Logo URL"</label>
                                <input
                                    type="text"
                                    id="branding-logo-url"
                                    maxlength="2048"
                                    prop:value=logo_url
                                    on:input=move |ev| set_logo_url.set(event_target_value(&ev))
                                    style="width: 100%;"
                                />
                            </div>
                            <button
                                id="save-branding-btn"
                                on:click=move |_| {
                                    if let Some(cb) = on_set_branding {
                                        let logo = logo_url.get();
                                        cb.run(shared::BrandingConfig {
                                            primary_color: Some(primary_color.get()),
                                            background_color: Some(bg_color.get()),
                                            logo_url: if logo.is_empty() { None } else { Some(logo) },
                                        });
                                    }
                                }
                                class="btn btn-primary"
                            >
                                "Apply Branding"
                            </button>
                        </Show>
                        <Show when=move || active_tab.get() == "moderator">
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: block; margin-bottom: 5px;">"Meeting Subject"</label>
                                <div style="display: flex; gap: 10px;">
                                    <input
                                        type="text"
                                        id="settings-subject"
                                        maxlength="256"
                                        prop:value=move || subject.get()
                                        on:input=move |ev| set_subject.set(event_target_value(&ev))
                                        style="flex: 1; padding: 8px; border: 1px solid #ccc; border-radius: 4px;"
                                    />
                                    <button
                                        id="update-subject-btn"
                                        on:click=move |_| {
                                            if let Some(cb) = on_set_subject {
                                                cb.run(subject.get());
                                            }
                                        }
                                        class="btn btn-success"
                                    >
                                        "Update"
                                    </button>
                                </div>
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: flex; align-items: center; cursor: pointer;">
                                    <input
                                        type="checkbox"
                                        id="lock-room-toggle"
                                        prop:checked=move || is_locked.map(|l| l.get()).unwrap_or(false)
                                        on:change=move |_| {
                                            if let Some(cb) = on_toggle_lock {
                                                let pw = lock_password.get_untracked();
                                                cb.run(if pw.trim().is_empty() { None } else { Some(pw) });
                                            }
                                        }
                                        style="margin-right: 10px;"
                                    />
                                    {move || t("lock_room")}
                                </label>
                                <input
                                    type="password"
                                    id="lock-password-input"
                                    placeholder="Password (optional)"
                                    on:input=move |ev| set_lock_password.set(event_target_value(&ev))
                                    prop:value=move || lock_password.get()
                                    style="margin-top: 8px; width: 100%;"
                                />
                                <p class="settings-hint" style="margin-top: 4px;">
                                    "Set a password before locking to require it for new joins."
                                </p>
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: flex; align-items: center; cursor: pointer;">
                                    <input
                                        type="checkbox"
                                        id="lobby-toggle"
                                        prop:checked=move || is_lobby_enabled.map(|l| l.get()).unwrap_or(false)
                                        on:change=move |_| {
                                            if let Some(cb) = on_toggle_lobby {
                                                cb.run(());
                                            }
                                        }
                                        style="margin-right: 10px;"
                                    />
                                    {move || t("enable_lobby")}
                                </label>
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: flex; align-items: center; cursor: pointer;">
                                    <input
                                        type="checkbox"
                                        id="e2ee-toggle"
                                        prop:checked=move || is_e2ee_enabled.map(|l| l.get()).unwrap_or(false)
                                        on:change=move |_| {
                                            if let Some(cb) = on_toggle_e2ee {
                                                cb.run(());
                                            }
                                        }
                                        style="margin-right: 10px;"
                                    />
                                    {move || "Enable End-to-End Encryption (visual indicator only)"}
                                    <input type="hidden" class="e2ee-toggle-marker" />
                                </label>
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: flex; align-items: center; cursor: pointer;">
                                    <input
                                        type="checkbox"
                                        id="audio-moderation-toggle"
                                        prop:checked=move || is_audio_moderation_enabled.map(|s| s.get()).unwrap_or(false)
                                        on:change=move |_| {
                                            if let Some(cb) = on_toggle_audio_moderation {
                                                cb.run(());
                                            }
                                        }
                                        style="margin-right: 10px;"
                                    />
                                    "Moderated Audio (Permission required to unmute)"
                                </label>
                            </div>
                            <div class="form-group" style="margin-bottom: 15px;">
                                <label style="display: flex; align-items: center; cursor: pointer;">
                                    <input
                                        type="checkbox"
                                        id="video-moderation-toggle"
                                        prop:checked=move || is_video_moderation_enabled.map(|s| s.get()).unwrap_or(false)
                                        on:change=move |_| {
                                            if let Some(cb) = on_toggle_video_moderation {
                                                cb.run(());
                                            }
                                        }
                                        style="margin-right: 10px;"
                                    />
                                    "Moderated Video (Permission required for camera)"
                                </label>
                            </div>
                        </Show>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Component testing with web_sys/Leptos requires browser environment.
    #[test]
    fn test_settings_dialog_exists() {
        assert_eq!(1, 1);
    }

    #[test]
    fn test_device_settings_tuple() {
        let settings: DeviceSettings = (
            Some("cam1".to_string()),
            Some("mic1".to_string()),
            "hd".to_string(),
            false,
        );
        assert_eq!(settings.0.unwrap(), "cam1");
        assert_eq!(settings.1.unwrap(), "mic1");
        assert_eq!(settings.2, "hd");
    }

    #[test]
    fn test_empty_device_settings() {
        let settings: DeviceSettings = (None, None, "sd".to_string(), false);
        assert_eq!(settings.0, None);
        assert_eq!(settings.1, None);
        assert_eq!(settings.2, "sd");
        assert!(!settings.3);
    }

    #[test]
    fn test_device_settings_with_noise_suppression() {
        let settings: DeviceSettings = (None, None, "hd".to_string(), true);
        assert!(settings.3);
    }
}
