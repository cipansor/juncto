use leptos::prelude::*;

fn presence_label(status: &shared::PresenceStatus) -> &'static str {
    match status {
        shared::PresenceStatus::Connected => "Connected",
        shared::PresenceStatus::Busy => "Busy",
        shared::PresenceStatus::Calling => "Calling",
        shared::PresenceStatus::Ringing => "Ringing",
        _ => "Connected",
    }
}

fn presence_from_label(label: &str) -> shared::PresenceStatus {
    match label {
        "Connected" => shared::PresenceStatus::Connected,
        "Busy" => shared::PresenceStatus::Busy,
        "Calling" => shared::PresenceStatus::Calling,
        "Ringing" => shared::PresenceStatus::Ringing,
        _ => shared::PresenceStatus::Connected,
    }
}

#[component]
pub fn Toolbox(
    _is_locked: ReadSignal<bool>,
    is_host: Signal<bool>,
    is_visitor: Signal<bool>,
    #[prop(optional)] _is_lobby_enabled: Option<ReadSignal<bool>>,
    is_recording: ReadSignal<bool>,
    is_subtitles_enabled: ReadSignal<bool>,
    _on_toggle_e2ee: Callback<()>,
    _is_e2ee_enabled: ReadSignal<bool>,
    on_toggle_etherpad: Callback<()>,
    _is_etherpad_active: Signal<bool>,
    is_etherpad_open: Signal<bool>,
    current_presence: Signal<shared::PresenceStatus>,
    #[prop(optional)] _on_toggle_lock: Option<Callback<Option<String>>>,
    #[prop(optional)] _on_toggle_lobby: Option<Callback<()>>,
    on_toggle_recording: Callback<()>,
    on_toggle_subtitles: Callback<()>,
    on_set_presence: Callback<shared::PresenceStatus>,
    on_invite: Callback<()>,
    on_toggle_chat: Callback<()>,
    on_toggle_participants: Callback<()>,
    on_settings: Callback<()>,
    on_polls: Callback<()>,
    on_shortcuts: Callback<()>,
    on_speaker_stats: Callback<()>,
    #[prop(optional)] on_virtual_background: Option<Callback<()>>,
    on_feedback: Callback<()>,
    #[prop(optional)] on_embed: Option<Callback<()>>,
    on_raise_hand: Callback<()>,
    /// Whether *this* participant's hand is up. The button used to be a fixed
    /// "Raise Hand" label, so a raised hand looked like a click that did nothing.
    #[prop(optional)]
    is_hand_raised: Option<Signal<bool>>,
    on_screen_share: Callback<()>,
    on_share_video: Callback<()>,
    on_stop_share_video: Callback<()>,
    is_sharing_video: Signal<bool>,
    on_whiteboard: Callback<()>,
    on_reaction: Callback<String>,
    #[prop(optional)] is_audio_moderated: Option<Signal<bool>>,
    #[prop(optional)] is_video_moderated: Option<Signal<bool>>,
    #[prop(optional)] has_unmute_permission: Option<ReadSignal<bool>>,
    #[prop(optional)] has_camera_permission: Option<ReadSignal<bool>>,
    #[prop(optional)] on_request_unmute_permission: Option<Callback<()>>,
    #[prop(optional)] on_request_camera_permission: Option<Callback<()>>,
    on_toggle_camera: Callback<()>,
    on_toggle_mic: Callback<()>,
    is_muted: ReadSignal<bool>,
    on_auth_dialog: Callback<()>,
    #[prop(optional)] on_calendar: Option<Callback<()>>,
    on_files: Callback<()>,
    #[prop(optional)] on_dial_in: Option<Callback<()>>,
    #[prop(optional)] on_salesforce: Option<Callback<()>>,
    #[prop(optional)] on_leave: Option<Callback<()>>,
    #[prop(optional)] on_end_meeting: Option<Callback<()>>,
    #[prop(optional)] class: &'static str,
    #[prop(optional)] style: &'static str,
    #[prop(optional)] is_recording_locally: Option<ReadSignal<bool>>,
    #[prop(optional)] on_toggle_local_recording: Option<Callback<()>>,
    #[prop(optional)] is_talking_while_muted: Option<ReadSignal<bool>>,
) -> impl IntoView {
    view! {
        <div class=format!("toolbox room-toolbox {}", class) style=style>
            // Group: Leave
            <div class="toolbox-group">
                <button
                    on:click=move |_| { if let Some(cb) = on_leave { cb.run(()); } }
                    class="btn btn-danger"
                    title="Leave Meeting"
                >
                    "Leave"
                </button>
                <Show when=move || is_host.get()>
                    <button
                        on:click=move |_| { if let Some(cb) = on_end_meeting { cb.run(()); } }
                        class="btn btn-outline"
                        style="color: var(--danger-color); border-color: var(--danger-color);"
                        title="End Meeting for Everyone"
                    >
                        "End Meeting"
                    </button>
                </Show>
                <button
                    on:click=move |_| on_invite.run(())
                    class="btn btn-outline"
                    title="Invite Others"
                >
                    "Invite"
                </button>
            </div>

            // Group: Media
            <div class="toolbox-group">
                <Show when=move || !is_visitor.get()>
                    <button
                        id="toggle-camera-btn"
                        on:click=move |_| on_toggle_camera.run(())
                        class="btn btn-outline"
                        title="Toggle Camera"
                    >
                        "Toggle Camera"
                    </button>
                    <button
                        id="toggle-mic-btn"
                        on:click=move |_| on_toggle_mic.run(())
                        class=move || format!("btn {} {}",
                            if is_muted.get() { "btn-danger" } else { "btn-success" },
                            if is_talking_while_muted.map(|s| s.get()).unwrap_or(false) { "pulse-animation" } else { "" }
                        )
                        title=move || if is_muted.get() { "Unmute" } else { "Mute" }
                    >
                        {move || if is_muted.get() { "Unmute" } else { "Mute" }}
                    </button>
                </Show>
                <Show when=move || !is_visitor.get() && !is_host.get()>
                    <Show when=move || is_audio_moderated.map(|s| s.get()).unwrap_or(false) && !has_unmute_permission.map(|s| s.get()).unwrap_or(false)>
                        <button
                            id="request-unmute-btn"
                            on:click=move |_| { if let Some(cb) = on_request_unmute_permission { cb.run(()); } }
                            class="btn btn-outline"
                            title="Request Unmute Permission"
                        >
                            "Req Mic"
                        </button>
                    </Show>
                    <Show when=move || is_video_moderated.map(|s| s.get()).unwrap_or(false) && !has_camera_permission.map(|s| s.get()).unwrap_or(false)>
                        <button
                            id="request-camera-btn"
                            on:click=move |_| { if let Some(cb) = on_request_camera_permission { cb.run(()); } }
                            class="btn btn-outline"
                            title="Request Camera Permission"
                        >
                            "Req Cam"
                        </button>
                    </Show>
                </Show>
                <button
                    id="toggle-subtitles-btn"
                    on:click=move |_| on_toggle_subtitles.run(())
                    class=move || format!("btn {}", if is_subtitles_enabled.get() { "btn-primary" } else { "btn-outline" })
                    title="Toggle Subtitles"
                >
                    "CC"
                </button>
                <Show when=move || is_host.get()>
                    <button
                        on:click=move |_| on_toggle_recording.run(())
                        class=move || format!("btn {}", if is_recording.get() { "btn-danger" } else { "btn-outline" })
                        title="Toggle Server Recording"
                    >
                        {move || if is_recording.get() { "Stop Recording" } else { "Start Recording" }}
                    </button>
                </Show>
            </div>

            // Group: Actions
            <div class="toolbox-group">
                <Show when=move || !is_visitor.get()>
                    <button
                        on:click=move |_| on_screen_share.run(())
                        class="btn btn-outline"
                        title="Share Screen"
                    >
                        "Share Screen"
                    </button>
                    <button
                        id="raise-hand-btn"
                        on:click=move |_| on_raise_hand.run(())
                        class="btn btn-warning"
                        title=move || if is_hand_raised.map(|h| h.get()).unwrap_or(false) { "Lower Hand" } else { "Raise Hand" }
                    >
                        {move || if is_hand_raised.map(|h| h.get()).unwrap_or(false) { "Lower Hand" } else { "Raise Hand" }}
                    </button>
                </Show>
                <button
                    id="toggle-whiteboard-btn"
                    on:click=move |_| on_whiteboard.run(())
                    class="btn btn-outline"
                    title="Whiteboard"
                >
                    "Whiteboard"
                </button>
                <Show when=move || on_virtual_background.is_some() && !is_visitor.get()>
                    <button
                        on:click=move |_| on_virtual_background.unwrap().run(())
                        class="btn btn-outline"
                        title="Virtual Background"
                    >
                        "Background"
                    </button>
                </Show>
                <Show when=move || !is_visitor.get()>
                    <button
                        id="toggle-shared-video-btn"
                        on:click=move |_| {
                            if is_sharing_video.get() {
                                on_stop_share_video.run(());
                            } else {
                                on_share_video.run(());
                            }
                        }
                        class=move || format!("btn {}", if is_sharing_video.get() { "btn-danger" } else { "btn-outline" })
                        title="Share Video"
                    >
                        {move || if is_sharing_video.get() { "Stop Video" } else { "Share Video" }}
                    </button>
                </Show>
                <div class="reactions" style="display: flex; gap: 4px; align-items: center; margin-left: 5px;">
                    <button on:click=move |_| on_reaction.run("👍".to_string()) style="cursor: pointer; border: none; background: none; font-size: 1.2rem;">"👍"</button>
                    <button on:click=move |_| on_reaction.run("👏".to_string()) style="cursor: pointer; border: none; background: none; font-size: 1.2rem;">"👏"</button>
                </div>
            </div>

            // Group: Panels
            <div class="toolbox-group">
                <button
                    id="toggle-chat-btn"
                    on:click=move |_| on_toggle_chat.run(())
                    class="btn btn-outline"
                    title="Toggle Chat"
                >
                    "Chat"
                </button>
                <button
                    id="toggle-participants-btn"
                    on:click=move |_| on_toggle_participants.run(())
                    class="btn btn-outline"
                    title="Toggle Participants"
                >
                    "Participants"
                </button>
                <button
                    id="toggle-polls-btn"
                    on:click=move |_| on_polls.run(())
                    class="btn btn-outline"
                    title="Polls"
                >
                    "Polls"
                </button>
                <button
                    id="toggle-files-btn"
                    on:click=move |_| on_files.run(())
                    class="btn btn-outline"
                    title="Files"
                >
                    "Files"
                </button>
                <button
                    id="toggle-etherpad-btn"
                    on:click=move |_| on_toggle_etherpad.run(())
                    class=move || format!("btn {}", if is_etherpad_open.get() { "btn-primary" } else { "btn-outline" })
                    title="Shared Document (Etherpad)"
                >
                    "Etherpad"
                </button>
            </div>

            // Group: More
            <div class="toolbox-group" style="border-right: none;">
                <button
                    on:click=move |_| on_auth_dialog.run(())
                    class="btn btn-outline"
                    title="Login"
                >
                    "Login"
                </button>
                <button
                    on:click=move |_| on_speaker_stats.run(())
                    class="btn btn-outline"
                    title="Speaker Stats"
                >
                    "Stats"
                </button>
                <button
                    on:click=move |_| on_feedback.run(())
                    class="btn btn-outline"
                    title="Feedback"
                >
                    "Feedback"
                </button>
                <Show when=move || on_calendar.is_some()>
                    <button
                        on:click=move |_| on_calendar.unwrap().run(())
                        class="btn btn-outline"
                        title="Calendar"
                    >
                        "Calendar"
                    </button>
                </Show>
                <Show when=move || on_embed.is_some()>
                    <button
                        on:click=move |_| on_embed.unwrap().run(())
                        class="btn btn-outline"
                        title="Embed Meeting"
                    >
                        "Embed"
                    </button>
                </Show>
                <Show when=move || on_toggle_local_recording.is_some()>
                    <button
                        id="toggle-local-record-btn"
                        on:click=move |_| on_toggle_local_recording.unwrap().run(())
                        class=move || format!("btn {}", if is_recording_locally.map(|s| s.get()).unwrap_or(false) { "btn-danger" } else { "btn-outline" })
                        title="Local Record"
                    >
                        {move || if is_recording_locally.map(|s| s.get()).unwrap_or(false) { "Stop Local Rec" } else { "Local Record" }}
                    </button>
                </Show>
                <Show when=move || on_dial_in.is_some()>
                    <button
                        on:click=move |_| { if let Some(cb) = on_dial_in { cb.run(()); } }
                        class="btn btn-outline"
                        title="Dial-in Info"
                    >
                        "Dial"
                    </button>
                </Show>
                <Show when=move || on_salesforce.is_some() && is_host.get()>
                    <button
                        id="salesforce-btn"
                        on:click=move |_| { if let Some(cb) = on_salesforce { cb.run(()); } }
                        class="btn btn-outline"
                        title="Salesforce Integration"
                    >
                        "SF"
                    </button>
                </Show>
                <button
                    on:click=move |_| on_shortcuts.run(())
                    class="btn btn-outline"
                    title="Keyboard Shortcuts"
                >
                    "?"
                </button>
                <button
                    id="settings-btn"
                    on:click=move |_| on_settings.run(())
                    class="btn btn-outline"
                    title="Settings"
                >
                    "Settings"
                </button>
                <div class="presence-selector" style="display: flex; gap: 5px; align-items: center;">
                    <select
                        id="presence-select"
                        prop:value=move || presence_label(&current_presence.get())
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            on_set_presence.run(presence_from_label(&value));
                        }
                        style="padding: 4px; border-radius: 4px; border: 1px solid var(--border-color); background: var(--card-bg); color: white; font-size: 0.8rem;"
                    >
                        <option value="Connected">"Online"</option>
                        <option value="Busy">"Busy"</option>
                        <option value="Calling">"Calling"</option>
                    </select>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presence_label_roundtrip() {
        for status in [
            shared::PresenceStatus::Connected,
            shared::PresenceStatus::Busy,
            shared::PresenceStatus::Calling,
            shared::PresenceStatus::Ringing,
        ] {
            assert_eq!(presence_from_label(presence_label(&status)), status);
        }
        assert_eq!(
            presence_from_label("unknown"),
            shared::PresenceStatus::Connected
        );
    }
}
