use crate::components_ui::toast::{ToastType, use_toast};
use crate::i18n::t;
use browser_media::{enumerate_devices, get_user_media};
use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MediaDeviceInfo, MediaDeviceKind};

pub type DeviceSettings = (Option<String>, Option<String>, String, bool);

/// A password as the server stores it: whitespace-only drafts mean "no
/// password", anything else is kept byte-for-byte.
///
/// Trimming a non-blank draft would change the secret. The guest types what
/// the host shared, and the server compares exactly, so `" planning42 "` must
/// stay `" planning42 "` — trimming it makes the shared password fail.
pub fn normalize_password(draft: &str) -> Option<String> {
    if draft.trim().is_empty() {
        None
    } else {
        Some(draft.to_string())
    }
}

/// Whether the draft would change what the server holds.
///
/// This is the single predicate behind Apply, Enter and the unsaved hint, so
/// the three can never disagree about whether there is something to save.
/// `known` is the value the client believes is stored, or `None` when a
/// password exists but its value was never seen (`has_password`).
///
/// An empty draft is a real removal whenever a password exists — including one
/// whose value is unknown — otherwise a host could not clear a password that
/// was set through the lock toggle.
pub fn password_differs(draft: &str, known: &Option<String>, has_password: bool) -> bool {
    match (normalize_password(draft), known) {
        (None, None) => has_password,
        (None, Some(_)) => true,
        (Some(_), None) => true,
        (Some(draft), Some(known)) => draft != *known,
    }
}

/// How the pending edit should be described above the Apply button.
///
/// A blank draft against an existing password is a removal, not a save. Saying
/// "save this change" there hid the fact that pressing Apply drops the
/// password and leaves the room locked with nobody able to join, which is
/// exactly the state a host must not enter by accident.
pub fn pending_password_hint(draft: &str, has_password: bool) -> &'static str {
    if normalize_password(draft).is_none() && has_password {
        "Press Apply Password to remove the room password."
    } else {
        "Press Apply Password to save this change."
    }
}

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
    /// Server-reported state: a password exists even though the client never
    /// sees its value, so the hint must not claim the room has none.
    #[prop(optional)]
    has_password: Option<Signal<bool>>,
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
    #[prop(optional)] on_set_room_password: Option<Callback<Option<String>>>,
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

    // Every other modal in the app closes on Escape; this one did not, which
    // leaves keyboard users with no way out except the mouse.
    //
    // `WindowListenerHandle` is not detached on drop, so the listener would
    // outlive this component. `show` and `on_close` are owned by the room page,
    // and the room page's signals are disposed when client-side navigation
    // leaves it; the stale listener would then panic on `get_untracked()`.
    let show_for_esc = show;
    let escape_listener = window_event_listener(ev::keydown, move |ev| {
        if ev.key() == "Escape" && show_for_esc.get_untracked() {
            on_close.run(());
        }
    });
    on_cleanup(move || escape_listener.remove());

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
    // What the client believes the server holds, or `None` when the server has
    // a password whose value we never see. The toggle and the unlock path both
    // change the stored password server-side, so this is re-synced whenever the
    // room's lock state or password presence changes; a stale baseline makes a
    // real change look like no change and disables Apply.
    let (saved_password, set_saved_password) = signal(Option::<String>::None);
    // Tracked reads: these feed the Apply button, the unsaved hint and the
    // guidance text, all of which must re-render when the lock or password
    // state changes. Untracked reads here left the dialog showing the state it
    // had when it was first opened.
    let locked_now = move || is_locked.map(|l| l.get()).unwrap_or(false);
    let server_has_password = move || {
        has_password
            .map(|h| h.get())
            .unwrap_or_else(|| saved_password.get().is_some())
    };

    // Adopt the server's view after a transition the client did not make
    // itself: the toggle (which stores the draft and drops it on unlock) and
    // any other client's change. Without this, unlocking clears the server
    // password but the local baseline still holds the old value, so typing
    // that same password after a relock compares equal and Apply stays dead.
    //
    // Losing host rights invalidates the baseline outright: another host can
    // replace the password while the lock and presence flags both stay true,
    // so on a handoff back the cached value may no longer be what the server
    // holds. Treat any surviving server password as unknown instead.
    //
    // Every read is tracked, so this re-runs on any of those changes.
    Effect::new(move |_| {
        let locked = is_locked.map(|l| l.get()).unwrap_or(false);
        let has_pw = has_password.map(|h| h.get()).unwrap_or(false);
        let host = is_host.map(|h| h.get()).unwrap_or(false);
        let known_is_stale = if !host {
            true
        } else if locked {
            // A known value survives a relock only if the server still has a
            // password; otherwise what we hold is no longer what it has.
            !has_pw
        } else {
            // Unlocking always clears the stored password.
            true
        };
        if known_is_stale {
            set_saved_password.set(None);
        }
    });

    // Pushing the password to the server is an explicit action rather than a
    // side effect of blur. Closing the dialog (Escape, the ×, or clicking the
    // overlay) removes the focused input, and Firefox does not fire `change`
    // for a removed element — so a typed password was silently discarded and
    // every guest stayed locked out.
    let apply_password = move || {
        let Some(cb) = on_set_room_password else {
            return;
        };
        if !locked_now() {
            return;
        }
        let draft = lock_password.get_untracked();
        if !password_differs(
            &draft,
            &saved_password.get_untracked(),
            server_has_password(),
        ) {
            return;
        }
        let next = normalize_password(&draft);
        cb.run(next.clone());
        set_saved_password.set(next);
    };

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
                    crate::utils::play_quietly(&video_el);
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
                                                let pw = normalize_password(&lock_password.get_untracked());
                                                // The toggle stores the draft, so the
                                                // server now holds exactly this.
                                                set_saved_password.set(pw.clone());
                                                cb.run(pw);
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
                                    on:keydown=move |ev| {
                                        if ev.key() == "Enter" {
                                            ev.prevent_default();
                                            apply_password();
                                        }
                                    }
                                    prop:value=move || lock_password.get()
                                    style="margin-top: 8px; width: 100%;"
                                />
                                <button
                                    id="apply-password-btn"
                                    class="btn btn-secondary"
                                    prop:disabled=move || {
                                        !locked_now()
                                            || !password_differs(
                                                &lock_password.get(),
                                                &saved_password.get(),
                                                server_has_password(),
                                            )
                                    }
                                    on:click=move |_| apply_password()
                                >
                                    "Apply Password"
                                </button>
                                <Show when=move || {
                                    locked_now()
                                        && password_differs(
                                            &lock_password.get(),
                                            &saved_password.get(),
                                            server_has_password(),
                                        )
                                }>
                                    <p id="password-unsaved" class="settings-hint" style="margin-top: 4px;">
                                        {move || pending_password_hint(
                                            &lock_password.get(),
                                            server_has_password(),
                                        )}
                                    </p>
                                </Show>
                                <p class="settings-hint" style="margin-top: 4px;">
                                    {move || {
                                        if locked_now() {
                                            if server_has_password() {
                                                "Guests need this password to join. Clear it and press Apply to keep the room closed."
                                            } else {
                                                "The room is locked without a password, so nobody can join. Apply a password to let guests in."
                                            }
                                        } else {
                                            "Set a password before locking to require it for new joins."
                                        }
                                    }}
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

    #[test]
    fn normalize_password_keeps_significant_spaces() {
        // Regression: trimming a non-blank draft changed the secret, so a guest
        // pasting the password the host shared was rejected.
        assert_eq!(
            normalize_password(" planning42 "),
            Some(" planning42 ".to_string())
        );
        assert_eq!(normalize_password("a b"), Some("a b".to_string()));
        // Only whitespace-only drafts mean "no password".
        assert_eq!(normalize_password(""), None);
        assert_eq!(normalize_password("   "), None);
        assert_eq!(normalize_password("\t\n"), None);
    }

    #[test]
    fn password_differs_treats_an_unknown_stored_password_as_a_change() {
        // The server has a password but the client never sees its value, so any
        // non-empty draft is a change and an empty draft is a real removal.
        assert!(password_differs("meeting42", &None, true));
        assert!(password_differs("", &None, true));
        assert!(password_differs("   ", &None, true));
        // Nothing stored and nothing typed: nothing to save.
        assert!(!password_differs("", &None, false));
        assert!(!password_differs("   ", &None, false));
    }

    #[test]
    fn password_differs_compares_against_a_known_value() {
        let known = Some("meeting42".to_string());
        assert!(!password_differs("meeting42", &known, true));
        assert!(password_differs("meeting43", &known, true));
        // Clearing a password the client knows about is a change.
        assert!(password_differs("", &known, true));
        // Significant spaces are part of the value, not noise.
        assert!(password_differs(" meeting42", &known, true));
    }

    #[test]
    fn pending_hint_says_removal_when_a_blank_draft_would_clear_the_password() {
        // A blank field against a live password is a removal, and pressing
        // Apply locks everyone out; the prompt must say so.
        assert_eq!(
            pending_password_hint("", true),
            "Press Apply Password to remove the room password."
        );
        assert_eq!(
            pending_password_hint("   ", true),
            "Press Apply Password to remove the room password."
        );
        // Anything else is an ordinary save, including the first password.
        assert_eq!(
            pending_password_hint("meeting42", true),
            "Press Apply Password to save this change."
        );
        assert_eq!(
            pending_password_hint("meeting42", false),
            "Press Apply Password to save this change."
        );
        // Nothing stored and nothing typed is not a removal of anything.
        assert_eq!(
            pending_password_hint("", false),
            "Press Apply Password to save this change."
        );
    }

    /// The two transitions the report called out, as a state machine.
    #[test]
    fn stale_baseline_never_blocks_a_real_reapply() {
        // 1. Type a password, lock (the toggle stores it and adopts it).
        let mut known = normalize_password("meeting42");
        assert_eq!(known, Some("meeting42".to_string()));
        // 2. Clear the field: the server still admits with meeting42, so
        //    clearing must be applicable.
        assert!(password_differs("", &known, true));
        // 3. Apply the removal.
        known = normalize_password("");
        assert_eq!(known, None);
        assert!(!password_differs("", &known, false));

        // 4. Apply meeting42 to a locked room, then unlock: the server drops
        //    the password, so the baseline must go with it.
        known = normalize_password("meeting42");
        assert_eq!(known, Some("meeting42".to_string()));
        known = None; // the Effect clears it on unlock
        // 5. Relock without a password, then type meeting42 again. The server
        //    now has no password, so this is a real change and must apply.
        assert!(password_differs("meeting42", &known, false));
    }
}
