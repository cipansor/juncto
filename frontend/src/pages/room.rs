use crate::chat::Chat;
use crate::components_ui::always_on_top::AlwaysOnTop;
use crate::components_ui::authentication::LoginDialog;
use crate::components_ui::breakout::BreakoutRooms;
use crate::components_ui::calendar::CalendarList;
use crate::components_ui::connection_indicator::ConnectionIndicator;
use crate::components_ui::dial_in::DialInDialog;
use crate::components_ui::feedback::FeedbackDialog;
use crate::components_ui::invite::InviteDialog;
use crate::components_ui::lobby::LobbyScreen;
use crate::components_ui::prejoin::PrejoinScreen;
use crate::components_ui::rejoin_overlay::RejoinOverlay;
use crate::components_ui::screenshot_capture::ScreenshotCapture;
use crate::components_ui::shared_video_dialog::SharedVideoDialog;
use crate::components_ui::toast::NotificationBell;
use crate::components_ui::video_grid::VideoGrid;
use crate::connection_stats::ConnectionStats;
use crate::participants::ParticipantsList;
use crate::polls::PollsDialog;
use crate::reactions::ReactionDisplay;
use crate::salesforce::LinkSalesforceDialog;
use crate::settings::SettingsDialog;
use crate::shortcuts::{KeyboardShortcuts, ShortcutsDialog};
use crate::speaker_stats::SpeakerStatsDialog;
use crate::state::{RoomConnectionState, use_room_state};
use crate::toolbox::Toolbox;
use crate::virtual_background::VirtualBackgroundDialog;
use crate::whiteboard::Whiteboard;
use gloo_timers::callback::Interval;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use wasm_bindgen::JsCast;

use crate::cleanup::on_cleanup_local;
use crate::deeplink::DeepLinking;
use crate::power_monitor::PowerMonitor;
use crate::remote_control::RemoteControlLayer;

#[component]
pub fn Room() -> impl IntoView {
    let params = use_params_map();
    let room_id = move || {
        params.with(|params| {
            let id = params.get("id").unwrap_or_default();
            urlencoding::decode(&id)
                .map(|s| s.into_owned())
                .unwrap_or(id)
        })
    };

    let state = use_room_state();
    let (show_shared_video_dialog, set_show_shared_video_dialog) = signal(false);
    let (show_invite, set_show_invite) = signal(false);
    let (show_embed, set_show_embed) = signal(false);
    // Side panels are mutually exclusive; at most one is open at a time to
    // avoid squeezing/overlapping the stage (esp. on narrow viewports).
    // Open chat by default on desktop, but start closed on narrow viewports
    // where the panel is full-width and would hide the stage behind it.
    let narrow_viewport = web_sys::window()
        .and_then(|w| w.inner_width().ok())
        .and_then(|v| v.as_f64())
        .is_some_and(|w| w <= 768.0);
    let (active_panel, set_active_panel) =
        signal(if narrow_viewport { None } else { Some("chat") });
    let show_chat = Signal::derive(move || active_panel.get() == Some("chat"));
    let show_participants = Signal::derive(move || active_panel.get() == Some("participants"));
    let show_files = Signal::derive(move || active_panel.get() == Some("files"));
    let toggle_panel = move |kind: &'static str| {
        set_active_panel.update(|p| *p = if *p == Some(kind) { None } else { Some(kind) });
    };
    let toggle_chat_cb = Callback::new(move |_| toggle_panel("chat"));
    let toggle_participants_cb = Callback::new(move |_| toggle_panel("participants"));
    let toggle_files_cb = Callback::new(move |_| toggle_panel("files"));
    let (show_dial_in, set_show_dial_in) = signal(false);
    let (show_salesforce, set_show_salesforce) = signal(false);

    let invite_url = Signal::derive(move || {
        if let Some(window) = web_sys::window() {
            window.location().href().unwrap_or_default()
        } else {
            "".to_string()
        }
    });

    let state_clone = state.clone();
    let leave_room = Callback::new(move |_| {
        // Stop raw camera/mic tracks first to release hardware immediately.
        // When a virtual background is active, local_stream holds canvas video
        // tracks (not the real getUserMedia tracks), so stopping only
        // local_stream would leak the camera.
        if let Some(raw) = state_clone.raw_local_stream.get_untracked() {
            let tracks = raw.get_tracks();
            for i in 0..tracks.length() {
                if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                    track.stop();
                }
            }
        }
        // Also stop processed stream tracks (canvas video tracks) for cleanup
        if let Some(stream) = state_clone.local_stream.get_untracked() {
            let tracks = stream.get_tracks();
            for i in 0..tracks.length() {
                if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                    track.stop();
                }
            }
        }
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href("/");
        }
    });

    let state_end_meeting = state.end_meeting;
    let end_meeting_and_leave = Callback::new(move |_| {
        state_end_meeting.run(());
        set_timeout(
            move || {
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href("/");
                }
            },
            std::time::Duration::from_millis(1000), // Increase buffer to ensure server has time to broadcast RoomEnded
        );
    });

    // Meeting Timer
    let (elapsed_time, set_elapsed_time) = signal(0u32);
    Effect::new(move |_| {
        let handle = Interval::new(1000, move || {
            set_elapsed_time.update(|t| *t += 1);
        });
        on_cleanup_local(move || drop(handle));
    });

    let format_time = move || {
        let t = elapsed_time.get();
        let h = t / 3600;
        let m = (t % 3600) / 60;
        let s = t % 60;
        format!("{:02}:{:02}:{:02}", h, m, s)
    };

    view! {
        <div class="room-wrapper">
            {move || match state.connection_state.get() {
                RoomConnectionState::Prejoin => view! {
                    <PrejoinScreen
                        on_join=state.join_meeting
                        is_connected=state.is_connected
                        subject=Signal::derive({
                            let state = state.clone();
                            move || state.room_config.get().subject.clone()
                        })
                        password_required=state.password_required
                    />
                }.into_any(),
                RoomConnectionState::Lobby => view! {
                    <LobbyScreen announcement=state.lobby_announcement />
                }.into_any(),
                RoomConnectionState::Joined => view! {
                    <div class="room-container">
                        <RemoteControlLayer />
                        <PowerMonitor on_update=state.update_power_status />
                        <DeepLinking />
                        <ScreenshotCapture />
                        <RejoinOverlay show=state.show_rejoin on_rejoin=state.rejoin />
                        <KeyboardShortcuts
                            on_toggle_mic=state.toggle_mic
                            on_toggle_camera=state.toggle_camera
                            on_raise_hand=state.toggle_raise_hand
                            on_screen_share=state.toggle_screen_share
                            on_toggle_chat=toggle_chat_cb
                            on_toggle_participants=toggle_participants_cb
                            on_toggle_local_recording=Callback::new({
                                let toggle = state.toggle_local_recording;
                                move |_| {
                                    let current = state.is_recording_locally.get_untracked();
                                    toggle.run(!current);
                                }
                            })
                        />
                        <ConnectionStats
                            on_ping=state.send_ping
                            rtt=state.rtt
                        />
                        <div class="main-content room-root" class:panel-open=move || active_panel.get().is_some()>
                            <Show when=move || state.show_login_dialog.get()>
                                <LoginDialog
                                    auth_error=state.auth_error
                                    on_login=state.authenticate
                                    on_cancel=Callback::new(move |_| {
                                        state.set_auth_error.set(None);
                                        state.set_show_login_dialog.set(false);
                                    })
                                />
                            </Show>
                            <Show when=move || state.show_calendar.get()>
                                <CalendarList
                                    events=state.calendar_events
                                    on_refresh=state.fetch_calendar
                                    on_close=Callback::new(move |_| state.set_show_calendar.set(false))
                                />
                            </Show>
                            <BreakoutRooms
                                breakout_rooms=state.breakout_rooms
                                current_room_id=state.current_room_id
                                is_host=state.is_host
                                on_create=state.create_breakout_room
                                on_remove=state.remove_breakout_room
                                on_rename=state.rename_breakout_room
                                on_join=state.join_breakout_room
                                on_close_all=state.close_all_breakout_rooms
                                on_auto_assign=state.auto_assign_to_breakout_rooms
                            />
                            <div class="room-main">
                                <div class="video-container flex-col" style=move || {
                                    if state.show_etherpad.get() {
                                        "height: 30%; border-bottom: 1px solid var(--border-color);"
                                    } else {
                                        "height: 100%;"
                                    }
                                }>
                                    <div id="capture-area" class="capture-area">
                                        <div class="room-header">
                                            <Show when=move || state.branding.get().logo_url.as_ref().is_some_and(|l| !l.is_empty())>
                                                <img
                                                    id="room-logo"
                                                    src=move || state.branding.get().logo_url.unwrap_or_default()
                                                    alt="Branding Logo"
                                                />
                                            </Show>
                                            <h2>{move || format!("Meeting Room: {}", room_id())}</h2>
                                            <Show when=move || state.room_config.get().subject.as_ref().is_some_and(|s| !s.is_empty())>
                                                <span id="meeting-subject" class="badge-info">
                                                    {move || state.room_config.get().subject.unwrap_or_default()}
                                                </span>
                                            </Show>
                                            <span class="meeting-timer">
                                                {format_time}
                                            </span>
                                            <ConnectionIndicator rtt=state.rtt />
                                            <Show when=move || !state.is_locked.get()>
                                                <span class="badge-success">"Unlocked"</span>
                                            </Show>
                                            <Show when=move || state.is_locked.get()>
                                                <span class="badge-danger">"Locked"</span>
                                            </Show>
                                        </div>
                                        <Show when=move || state.current_room_id.get().is_some()>
                                            <h4 class="breakout-heading">" (In Breakout Room)"</h4>
                                        </Show>
                                        <Show when=move || state.is_recording.get()>
                                            <div class="rec-indicator">
                                                "REC"
                                            </div>
                                        </Show>
                                        <Show when=move || state.is_e2ee_enabled.get()>
                                            <div id="e2ee-indicator" class="e2ee-indicator" title="End-to-End Encryption indicator only — actual E2EE is not yet implemented">
                                                "🔒 E2EE (indicator)"
                                            </div>
                                        </Show>
                                        <NotificationBell />
                                        <Show when=move || state.is_connected.get()>
                                            <AlwaysOnTop
                                                is_video_muted=Signal::derive(move || state.local_stream.get().is_none_or(|s| s.get_video_tracks().length() == 0))
                                                is_audio_muted=Signal::derive(move || state.is_muted.get())
                                                audio_level=state.audio_level.into()
                                                on_toggle_video=state.toggle_camera
                                                on_toggle_audio=state.toggle_mic
                                                on_leave=leave_room
                                            />
                                        </Show>
                                        <VideoGrid
                                            participants=state.participants
                                            local_stream=state.local_stream
                                            local_screen_stream=state.local_screen_stream
                                            my_audio_level=state.audio_level.into()
                                            my_id=state.my_id
                                            shared_video_url=state.shared_video_url
                                            speaking_peers=state.speaking_peers
                                            dominant_speaker=state.dominant_speaker
                                            remote_streams=state.remote_streams
                                            layout=state.grid_layout
                                            on_set_layout=state.set_grid_layout
                                            pinned_participant=state.pinned_participant
                                            is_audio_only=state.is_audio_only.into()
                                            is_flipped=state.is_flipped.into()
                                            on_pin_participant=state.pin_participant
                                            on_kick_participant=state.kick_participant
                                            participant_volumes=state.participant_volumes
                                            on_set_voltage=state.set_participant_volume
                                            is_host=state.is_host
                                        />
                                    </div>
                                </div>
                                <ReactionDisplay last_reaction=state.last_reaction />
                                <Show when=move || state.is_subtitles_enabled.get()>
                                    <div class="subtitles-overlay">
                                        <For
                                            each=move || state.subtitles.get()
                                            key=|(uid, text, ts)| format!("{}-{}-{}", uid, text, ts)
                                            children=move |(_uid, text, _ts)| {
                                                view! {
                                                    <div class="subtitle-line">{text}</div>
                                                }
                                            }
                                        />
                                        {move || if state.subtitles.get().is_empty() {
                                            "Subtitles are currently enabled. (Transcriptions will appear here)".to_string()
                                        } else {
                                            "".to_string()
                                        }}
                                    </div>
                                </Show>
                                <Show when=move || state.show_whiteboard.get()>
                                    <Whiteboard
                                        on_draw=state.send_draw
                                        history=state.whiteboard_history
                                        reset=state.whiteboard_reset
                                        append=state.whiteboard_append
                                        my_id=state.my_id
                                        is_visitor=state.is_visitor
                                    />
                                </Show>
                                <Show when=move || state.show_etherpad.get()>
                                    <div class="etherpad-wrapper">
                                        <crate::components_ui::etherpad::Etherpad
                                            url=Signal::derive(move || state.room_config.get().etherpad_url)
                                        />
                                    </div>
                                </Show>
                            </div>
                            <Toolbox
                                _is_locked=state.is_locked
                                is_host=state.is_host
                                is_visitor=state.is_visitor
                                _is_lobby_enabled=state.is_lobby_enabled
                                class="room-toolbox toolbox-wrapper"
                                is_recording=state.is_recording
                                _on_toggle_lock=state.toggle_lock
                                _on_toggle_lobby=state.toggle_lobby
                                on_toggle_recording=state.toggle_recording
                                is_subtitles_enabled=state.is_subtitles_enabled
                                on_toggle_subtitles=state.toggle_subtitles
                                _on_toggle_e2ee=state.toggle_e2ee
                                _is_e2ee_enabled=state.is_e2ee_enabled
                                on_toggle_etherpad=Callback::new({
                                    let state = state.clone();
                                    let room_id_fn = room_id;
                                    move |_| {
                                        let current = state.show_etherpad.get_untracked();
                                        if state.is_host.get_untracked() {
                                            if !current {
                                                // If no URL is set yet, configure one and send to server
                                                let has_url = state.room_config.get_untracked().etherpad_url.is_some();
                                                if !has_url {
                                                    let rid = room_id_fn();
                                                    let pad_url = format!("https://etherpad.org/p/juncto-{}", rid);
                                                    state.toggle_etherpad.run(Some(pad_url));
                                                }
                                                // Optimistically show the panel immediately
                                                state.set_show_etherpad.set(true);
                                            } else {
                                                // Just hide locally; the URL stays active for other participants.
                                                // To remove the shared document for everyone, the host
                                                // can use the settings or a dedicated "Remove Pad" action.
                                                state.set_show_etherpad.set(false);
                                            }
                                        } else {
                                            state.set_show_etherpad.set(!current);
                                        }
                                    }
                                })
                                _is_etherpad_active=Signal::derive(move || state.room_config.get().etherpad_url.is_some())
                                is_etherpad_open=Signal::derive(move || state.show_etherpad.get())
                                current_presence=Signal::derive(move || {
                                    if let Some(my_id) = state.my_id.get()
                                        && let Some(me) = state.participants.get().iter().find(|p| p.id == my_id) {
                                            return me.presence.clone();
                                        }
                                    shared::PresenceStatus::Connected
                                })
                                on_set_presence=state.set_presence
                                on_invite=Callback::new(move |_| set_show_invite.set(true))
                                on_toggle_chat=toggle_chat_cb
                                on_toggle_participants=toggle_participants_cb
                                on_settings=Callback::new(move |_| state.set_show_settings.set(true))
                                is_talking_while_muted=state.is_talking_while_muted
                                is_recording_locally=state.is_recording_locally
                                on_toggle_local_recording=Callback::new({
                                    let toggle = state.toggle_local_recording;
                                    move |_| {
                                        let current = state.is_recording_locally.get_untracked();
                                        toggle.run(!current);
                                    }
                                })
                                on_polls=Callback::new(move |_| state.set_show_polls.set(true))
                                on_shortcuts=Callback::new(move |_| state.set_show_shortcuts.set(true))
                                on_speaker_stats=Callback::new(move |_| state.set_show_speaker_stats.set(true))
                                on_virtual_background=Callback::new(move |_| state.set_show_virtual_background.set(true))
                                on_feedback=Callback::new(move |_| state.set_show_feedback.set(true))
                                on_embed=Callback::new(move |_| set_show_embed.set(true))
                                on_raise_hand=state.toggle_raise_hand
                                on_screen_share=state.toggle_screen_share
                                on_share_video=Callback::new(move |_| set_show_shared_video_dialog.set(true))
                                on_stop_share_video=state.stop_share_video
                                is_sharing_video=Signal::derive(move || state.shared_video_url.get().is_some())
                                on_whiteboard=Callback::new(move |_| state.set_show_whiteboard.update(|v| *v = !*v))
                                on_reaction=state.send_reaction
                                is_audio_moderated=state.is_audio_moderated
                                is_video_moderated=state.is_video_moderated
                                has_unmute_permission=state.has_unmute_permission
                                has_camera_permission=state.has_camera_permission
                                on_request_unmute_permission=state.request_unmute_permission
                                on_request_camera_permission=state.request_camera_permission
                                on_toggle_camera=state.toggle_camera
                                on_toggle_mic=state.toggle_mic
                                is_muted=state.is_muted
                                on_auth_dialog=Callback::new(move |_| {
                                    state.set_auth_error.set(None);
                                    state.set_show_login_dialog.set(true);
                                })
                                on_calendar=Callback::new(move |_| state.set_show_calendar.set(true))
                                on_files=toggle_files_cb
                                on_dial_in=Callback::new(move |_| set_show_dial_in.set(true))
                                on_salesforce=Callback::new(move |_| set_show_salesforce.set(true))
                                on_leave=leave_room
                                on_end_meeting=end_meeting_and_leave
                            />
                        </div>
                        <div
                            class="side-panel chat-container" id="chat-panel" class:panel-hidden=move || !show_chat.get()
                        >
                            <div class="panel-header">
                                <h3>"Chat"</h3>
                                <button class="close-btn" on:click=move |_| set_active_panel.set(None)>"×"</button>
                            </div>
                            <div class="panel-content p0">
                                <Chat
                                    messages=state.messages
                                    typing_users=state.typing_users
                                    participants=state.participants
                                    on_send=state.send_message
                                    on_typing=state.set_is_typing
                                    is_connected=state.is_connected
                                    my_id=state.my_id
                                    current_room_id=state.current_room_id
                                    is_visitor=state.is_visitor
                                />
                            </div>
                        </div>
                        <div
                            class="side-panel participants-container" id="participants-panel" class:panel-hidden=move || !show_participants.get()
                        >
                            <div class="panel-header">
                                <h3>"Participants"</h3>
                                <button class="close-btn" on:click=move |_| set_active_panel.set(None)>"×"</button>
                            </div>
                            <div class="panel-content p0">
                                <ParticipantsList
                                    participants=state.participants
                                    knocking_participants=state.knocking_participants
                                    host_id=state.host_id
                                    is_host=state.is_host
                                    my_id=state.my_id
                                    on_allow=state.grant_access
                                    on_deny=state.deny_access
                                    on_kick=state.kick_participant
                                    on_mute=state.mute_participant
                                    on_mute_camera=state.mute_camera_participant
                                    on_mute_all=state.mute_all
                                    on_mute_camera_all=state.mute_camera_all
                                    on_transfer_host=state.transfer_host
                                    _power_statuses=state.power_statuses
                                    on_request_unmute=state.request_unmute
                                    on_broadcast_lobby=state.broadcast_to_lobby
                                    on_promote=state.promote_visitor
                                    on_request_remote_control=Callback::new({
                                        let state = state.clone();
                                        move |id| state.remote_control.request_control(id)
                                    })
                                    on_stop_screen_share_all=state.stop_screen_share_all
                                        pinned_participant=state.pinned_participant
                                        on_pin=state.pin_participant
                                        on_set_volume=state.set_participant_volume
                                        on_mute_everyone_else=state.mute_everyone_else
                                        pending_unmute_requests=state.pending_unmute_requests
                                        pending_camera_requests=state.pending_camera_requests
                                        on_grant_unmute=state.grant_unmute_permission
                                        on_grant_camera=state.grant_camera_permission
                                />
                            </div>
                        </div>
                        <div class="side-panel files-container" class:panel-hidden=move || !show_files.get()>
                            <div class="panel-header">
                                <h3>"Files"</h3>
                                <button class="close-btn" on:click=move |_| set_active_panel.set(None)>"×"</button>
                            </div>
                            <div class="panel-content p0">
                                <crate::components_ui::file_sharing::FileSharing
                                    messages=state.messages
                                />
                            </div>
                        </div>
                        <InviteDialog
                            show=show_invite
                            on_close=Callback::new(move |_| set_show_invite.set(false))
                            room_url=invite_url
                        />
                        <SettingsDialog
                            show=state.show_settings
                            on_close=Callback::new(move |_| state.set_show_settings.set(false))
                            on_save_profile=state.save_profile
                            current_name=Signal::derive({
                                let state = state.clone();
                                move || {
                                    state.participants.get().iter().find(|p| Some(p.id.clone()) == state.my_id.get()).map(|p| p.name.clone()).unwrap_or_default()
                                }
                            })
                            current_avatar=Signal::derive({
                                let state = state.clone();
                                move || {
                                    state.participants.get().iter().find(|p| Some(p.id.clone()) == state.my_id.get()).and_then(|p| p.avatar_url.clone())
                                }
                            })
                            current_subject=Signal::derive({
                                let state = state.clone();
                                move || state.room_config.get().subject.clone()
                            })
                            on_save_avatar=state.set_avatar_url
                            on_set_subject=state.set_subject
                            on_save_devices=state.set_input_devices
                            current_video_id=state.selected_camera_id
                            current_audio_id=state.selected_mic_id
                            current_resolution=state.video_resolution
                            current_noise_suppression=state.is_noise_suppression_enabled
                            is_host=state.is_host
                            is_locked=state.is_locked
                            is_e2ee_enabled=state.is_e2ee_enabled
                            is_audio_moderation_enabled=Signal::derive({
                                let state = state.clone();
                                move || state.room_config.get().audio_moderation_enabled
                            })
                            is_video_moderation_enabled=Signal::derive({
                                let state = state.clone();
                                move || state.room_config.get().video_moderation_enabled
                            })
                            is_lobby_enabled=state.is_lobby_enabled
                            is_participant_e2ee_enabled=Signal::derive({
                                let state = state.clone();
                                move || {
                                    if let Some(me_id) = state.my_id.get() {
                                        state.participants.get().iter().find(|p| p.id == me_id).map(|p| p.e2ee_enabled).unwrap_or(false)
                                    } else {
                                        false
                                    }
                                }
                            })
                            is_face_landmarks_enabled=state.is_face_landmarks_enabled
                                is_audio_only=state.is_audio_only
                                is_flipped=state.is_flipped
                                on_toggle_audio_only=state.toggle_audio_only
                                on_toggle_flip=state.toggle_flip
                            on_toggle_lock=state.toggle_lock
                            on_toggle_e2ee=state.toggle_e2ee
                            on_toggle_audio_moderation=state.toggle_audio_moderation
                            on_toggle_video_moderation=state.toggle_video_moderation
                            on_toggle_participant_e2ee=state.toggle_participant_e2ee
                            on_toggle_lobby=state.toggle_lobby
                            on_set_branding=state.set_branding
                            current_branding=state.branding.into()
                        />
                        <SharedVideoDialog
                            show=show_shared_video_dialog
                            on_close=Callback::new(move |_| set_show_shared_video_dialog.set(false))
                            on_submit=state.start_share_video
                        />
                        <PollsDialog
                            show=state.show_polls
                            polls=state.polls
                            is_host=state.is_host
                            on_close=Callback::new(move |_| state.set_show_polls.set(false))
                            on_create_poll=state.create_poll
                            on_vote=state.vote_poll
                            on_close_poll=state.close_poll
                        />
                        <ShortcutsDialog
                            show=state.show_shortcuts
                            on_close=Callback::new(move |_| state.set_show_shortcuts.set(false))
                        />
                        <SpeakerStatsDialog
                            show=state.show_speaker_stats
                            participants=state.participants
                            on_close=Callback::new(move |_| state.set_show_speaker_stats.set(false))
                        />
                        <VirtualBackgroundDialog
                            show=state.show_virtual_background
                            on_close=Callback::new(move |_| state.set_show_virtual_background.set(false))
                            on_change=state.set_background_mode
                            current_mode=state.background_mode
                        />

                        <crate::components_ui::embed_meeting::EmbedMeetingDialog
                            show=show_embed
                            on_close=Callback::new(move |_| set_show_embed.set(false))
                        />
                        <FeedbackDialog
                            show=state.show_feedback
                            on_close=Callback::new(move |_| state.set_show_feedback.set(false))
                        />
                        <DialInDialog
                            show=show_dial_in
                            on_close=Callback::new(move |_| set_show_dial_in.set(false))
                        />
                        <LinkSalesforceDialog
                            show=show_salesforce
                            on_close=Callback::new(move |_| set_show_salesforce.set(false))
                            config=Signal::derive({
                                let state = state.clone();
                                move || state.room_config.get().salesforce.clone()
                            })
                        />
                    </div>
                }.into_any()
            }}
        </div>
    }
}
