use crate::analytics::{AnalyticsService, provide_analytics_context, use_analytics};
use crate::cleanup::on_cleanup_local;
use crate::components_ui::toast::{ToastType, use_toast};
use crate::dropbox::provide_dropbox_context;
use crate::face_landmarks::{
    FaceLandmarksService, provide_face_landmarks_context, use_face_landmarks,
};
use crate::media::{AudioMonitor, get_display_media, get_user_media};
use crate::remote_control::{
    RemoteControlService, provide_remote_control_context, use_remote_control,
};
use crate::salesforce::provide_salesforce_context;
use crate::state_handlers::{HandlerContext, handle_server_message};
use crate::storage::{load_settings, update_setting};
use crate::webrtc::WebRTCManager;
use leptos::prelude::*;
use leptos::task::spawn_local;
use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use shared::{
    ChatMessage, ClientMessage, DrawAction, FileAttachment, Participant, Poll, ServerMessage,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{MediaStream, MessageEvent, WebSocket};

#[derive(Clone, PartialEq, Debug)]
pub enum RoomConnectionState {
    Prejoin,
    Lobby,
    Joined,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JoinOptions {
    pub display_name: String,
    pub mic_enabled: bool,
    pub camera_enabled: bool,
    pub audio_device_id: Option<String>,
    pub video_device_id: Option<String>,
    pub is_visitor: bool,
    pub avatar_url: Option<String>,
    /// Access password for password-locked rooms.
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Clone)]
pub struct RoomState {
    pub connection_state: ReadSignal<RoomConnectionState>,
    pub messages: ReadSignal<Vec<ChatMessage>>,
    pub participants: ReadSignal<Vec<Participant>>,
    pub knocking_participants: ReadSignal<Vec<Participant>>,
    pub is_connected: ReadSignal<bool>,
    pub is_locked: ReadSignal<bool>,
    pub is_e2ee_enabled: ReadSignal<bool>,
    pub is_lobby_enabled: ReadSignal<bool>,
    pub is_recording: ReadSignal<bool>,
    pub is_subtitles_enabled: ReadSignal<bool>,
    pub subtitles: ReadSignal<Vec<(String, String, u64)>>,
    pub show_settings: ReadSignal<bool>,
    pub show_polls: ReadSignal<bool>,
    pub show_shortcuts: ReadSignal<bool>,
    pub polls: ReadSignal<Vec<Poll>>,
    pub last_reaction: ReadSignal<Option<(String, String, u64)>>,
    pub show_whiteboard: ReadSignal<bool>,
    pub show_etherpad: ReadSignal<bool>,
    pub whiteboard_history: ReadSignal<Vec<DrawAction>>,
    pub whiteboard_reset: ReadSignal<u32>,
    pub my_id: ReadSignal<Option<String>>,
    pub typing_users: ReadSignal<HashSet<String>>,
    pub is_host: Signal<bool>,
    pub host_id: Signal<Option<String>>,
    pub current_room_id: ReadSignal<Option<String>>,
    pub breakout_rooms: ReadSignal<Vec<shared::BreakoutRoom>>,
    #[allow(dead_code)]
    pub is_authenticated: ReadSignal<bool>,
    pub show_login_dialog: ReadSignal<bool>,
    pub auth_error: ReadSignal<Option<String>>,
    pub calendar_events: ReadSignal<Vec<String>>,
    pub show_calendar: ReadSignal<bool>,
    pub local_stream: ReadSignal<Option<MediaStream>>,
    pub local_screen_stream: ReadSignal<Option<MediaStream>>,
    pub is_muted: ReadSignal<bool>,
    pub shared_video_url: ReadSignal<Option<String>>,
    pub speaking_peers: ReadSignal<HashSet<String>>,
    #[allow(dead_code)]
    pub audio_monitor: ReadSignal<Option<AudioMonitor>>,
    #[allow(dead_code)]
    pub raw_local_stream: ReadSignal<Option<MediaStream>>,
    pub show_speaker_stats: ReadSignal<bool>,
    pub show_virtual_background: ReadSignal<bool>,
    pub show_feedback: ReadSignal<bool>,
    pub rtt: ReadSignal<u64>,
    pub audio_level: ReadSignal<f64>,
    pub remote_streams: ReadSignal<HashMap<String, Vec<MediaStream>>>,
    pub selected_camera_id: ReadSignal<Option<String>>,
    pub selected_mic_id: ReadSignal<Option<String>>,
    pub video_resolution: ReadSignal<String>,
    pub is_noise_suppression_enabled: ReadSignal<bool>,
    pub has_unmute_permission: ReadSignal<bool>,
    pub has_camera_permission: ReadSignal<bool>,
    pub pending_unmute_requests: ReadSignal<HashSet<String>>,
    pub pending_camera_requests: ReadSignal<HashSet<String>>,
    pub background_mode: ReadSignal<String>,
    pub grid_layout: ReadSignal<String>,
    pub is_visitor: Signal<bool>,
    pub room_config: ReadSignal<shared::RoomConfig>,
    pub power_statuses: ReadSignal<std::collections::HashMap<String, shared::PowerStatus>>,
    pub branding: ReadSignal<shared::BrandingConfig>,
    pub is_recording_locally: ReadSignal<bool>,
    pub is_talking_while_muted: ReadSignal<bool>,
    pub show_rejoin: ReadSignal<bool>,
    pub lobby_announcement: ReadSignal<Option<String>>,
    pub dominant_speaker: ReadSignal<Option<String>>,
    pub _last_face_expression: ReadSignal<Option<(String, String, u64)>>,
    pub is_face_landmarks_enabled: RwSignal<bool>,
    pub is_audio_only: ReadSignal<bool>,
    pub is_flipped: ReadSignal<bool>,
    pub pinned_participant: ReadSignal<Option<String>>,
    pub participant_volumes: ReadSignal<HashMap<String, f64>>,
    // Setters or Actions
    pub toggle_audio_only: Callback<bool>,
    pub toggle_flip: Callback<bool>,
    pub pin_participant: Callback<Option<String>>,
    pub set_participant_volume: Callback<(String, f64)>,
    pub mute_everyone_else: Callback<String>,
    pub request_unmute_permission: Callback<()>,
    pub request_camera_permission: Callback<()>,
    pub grant_unmute_permission: Callback<String>,
    pub grant_camera_permission: Callback<String>,
    pub toggle_audio_moderation: Callback<()>,
    pub toggle_video_moderation: Callback<()>,
    pub set_input_devices: Callback<(Option<String>, Option<String>, String, bool)>,
    pub set_background_mode: Callback<String>,
    pub set_grid_layout: Callback<String>,
    pub set_show_settings: WriteSignal<bool>,
    pub set_show_polls: WriteSignal<bool>,
    pub set_show_shortcuts: WriteSignal<bool>,
    pub set_show_whiteboard: WriteSignal<bool>,
    pub set_show_etherpad: WriteSignal<bool>,
    pub set_show_speaker_stats: WriteSignal<bool>,
    pub set_show_virtual_background: WriteSignal<bool>,
    pub set_show_feedback: WriteSignal<bool>,
    pub set_show_login_dialog: WriteSignal<bool>,
    pub set_auth_error: WriteSignal<Option<String>>,
    pub authenticate: Callback<(String, Option<String>)>,
    pub set_show_calendar: WriteSignal<bool>,
    pub fetch_calendar: Callback<()>,
    pub send_ping: Callback<()>,
    pub send_message: crate::chat::ChatSendCallback, // content, recipient_id, attachment
    pub start_share_video: Callback<String>,
    pub stop_share_video: Callback<()>,
    pub toggle_lock: Callback<Option<String>>,
    /// True when the server rejected the join because the room is locked
    /// with a password. The prejoin UI shows a password field in that case.
    pub password_required: ReadSignal<bool>,
    pub toggle_e2ee: Callback<()>,
    pub toggle_participant_e2ee: Callback<bool>,
    pub toggle_etherpad: Callback<Option<String>>,
    pub toggle_lobby: Callback<()>,
    pub toggle_recording: Callback<()>,
    pub toggle_subtitles: Callback<()>,
    pub grant_access: Callback<String>,
    pub deny_access: Callback<String>,
    pub join_meeting: Callback<JoinOptions>,
    pub rejoin: Callback<()>,
    pub save_profile: Callback<String>,
    pub set_avatar_url: Callback<Option<String>>,
    pub set_subject: Callback<String>,
    pub send_reaction: Callback<String>,
    pub toggle_raise_hand: Callback<()>,
    pub toggle_screen_share: Callback<()>,
    pub kick_participant: Callback<String>,
    pub create_poll: Callback<Poll>,
    pub vote_poll: Callback<(String, u32)>,
    pub close_poll: Callback<String>,
    pub send_draw: Callback<DrawAction>,
    pub set_is_typing: Callback<bool>,
    pub create_breakout_room: Callback<String>,
    pub remove_breakout_room: Callback<String>,
    pub rename_breakout_room: Callback<(String, String)>,
    pub join_breakout_room: Callback<Option<String>>,
    pub close_all_breakout_rooms: Callback<()>,
    #[allow(dead_code)]
    pub move_participant_to_room: Callback<(String, Option<String>)>,
    pub auto_assign_to_breakout_rooms: Callback<()>,
    pub toggle_camera: Callback<()>,
    pub toggle_mic: Callback<()>,
    pub end_meeting: Callback<()>,
    pub mute_participant: Callback<String>,
    pub mute_camera_participant: Callback<String>,
    pub mute_all: Callback<()>,
    pub mute_camera_all: Callback<()>,
    pub stop_screen_share_all: Callback<()>,
    pub set_branding: Callback<shared::BrandingConfig>,
    pub transfer_host: Callback<String>,
    pub set_presence: Callback<shared::PresenceStatus>,
    pub toggle_local_recording: Callback<bool>,
    pub request_unmute: Callback<String>,
    pub update_power_status: Callback<shared::PowerStatus>,
    pub broadcast_to_lobby: Callback<String>,
    pub promote_visitor: Callback<String>,
    #[allow(dead_code)]
    pub analytics: AnalyticsService,
    pub _face_landmarks: FaceLandmarksService,
    pub is_audio_moderated: Signal<bool>,
    pub is_video_moderated: Signal<bool>,
    pub remote_control: RemoteControlService,
}

pub fn use_room_state() -> RoomState {
    let settings = load_settings();
    let toast_ctx = use_toast();

    let (current_state, set_current_state) = signal(RoomConnectionState::Prejoin);
    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());
    let (typing_users, set_typing_users) = signal(HashSet::<String>::new());
    let (breakout_rooms, set_breakout_rooms) = signal(Vec::<shared::BreakoutRoom>::new());
    let (current_room_id, set_current_room_id) = signal(None::<String>);
    let (participants, set_participants) = signal(Vec::<Participant>::new());
    let (knocking_participants, set_knocking_participants) = signal(Vec::<Participant>::new());

    let (ws, set_ws) = signal(None::<WebSocket>);
    let (is_connected, set_is_connected) = signal(false);
    let (is_locked, set_is_locked) = signal(false);
    let (password_required, set_password_required) = signal(false);
    let (is_e2ee_enabled, set_is_e2ee_enabled) = signal(false);
    let (_e2ee_key, set_e2ee_key) = signal(None::<String>);
    let (is_lobby_enabled, set_is_lobby_enabled) = signal(false);
    let (is_recording, set_is_recording) = signal(false);
    let (is_subtitles_enabled, set_is_subtitles_enabled) = signal(false);
    let (subtitles, set_subtitles) = signal(Vec::<(String, String, u64)>::new());
    let (show_settings, set_show_settings) = signal(false);
    let (is_authenticated, set_is_authenticated) = signal(false);
    let (show_login_dialog, set_show_login_dialog) = signal(false);
    let (auth_error, set_auth_error) = signal(None::<String>);
    let (calendar_events, set_calendar_events) = signal(Vec::<String>::new());
    let (show_calendar, set_show_calendar) = signal(false);
    let (show_polls, set_show_polls) = signal(false);
    let (show_shortcuts, set_show_shortcuts) = signal(false);
    let (polls, set_polls) = signal(Vec::<Poll>::new());
    let (last_reaction, set_last_reaction) = signal(None::<(String, String, u64)>);
    let (show_whiteboard, set_show_whiteboard) = signal(false);
    let (show_etherpad, set_show_etherpad) = signal(false);
    let (whiteboard_history, set_whiteboard_history) = signal(Vec::<DrawAction>::new());
    let (_last_draw_action, set_last_draw_action) = signal(None::<DrawAction>);
    // Bumped whenever the server sends an authoritative whiteboard snapshot
    // (e.g. after it evicted old strokes). The canvas uses it to force a full
    // clear-and-repaint, since erased strokes cannot be removed incrementally.
    let (whiteboard_reset, set_whiteboard_reset) = signal(0u32);
    let (my_id, set_my_id) = signal(None::<String>);
    let (local_stream, set_local_stream) = signal(None::<MediaStream>);
    let (local_screen_stream, set_local_screen_stream) = signal(None::<MediaStream>);
    let (is_muted, set_is_muted) = signal(false);
    let (is_camera_off, set_is_camera_off) = signal(false);
    let (shared_video_url, set_shared_video_url) = signal(None::<String>);
    let (speaking_peers, set_speaking_peers) = signal(HashSet::<String>::new());
    let (audio_monitor, set_audio_monitor) = signal(None::<AudioMonitor>);
    let (show_speaker_stats, set_show_speaker_stats) = signal(false);
    let (show_virtual_background, set_show_virtual_background) = signal(false);
    let (show_feedback, set_show_feedback) = signal(false);
    let (rtt, set_rtt) = signal(0u64);
    let (audio_level, set_audio_level) = signal(0.0);
    let (last_ping_time, set_last_ping_time) = signal(0f64);
    let (selected_camera_id, set_selected_camera_id) = signal(settings.camera_id);
    let (selected_mic_id, set_selected_mic_id) = signal(settings.mic_id);
    let (video_resolution, set_video_resolution) =
        signal(settings.resolution.unwrap_or("hd".to_string()));
    let (is_noise_suppression_enabled, set_is_noise_suppression_enabled) = signal(false);
    let (has_unmute_permission, set_has_unmute_permission) = signal(false);
    let (has_camera_permission, set_has_camera_permission) = signal(false);
    let (pending_unmute_requests, set_pending_unmute_requests) = signal(HashSet::new());
    let (pending_camera_requests, set_pending_camera_requests) = signal(HashSet::new());
    let (background_mode, set_background_mode_sig) = signal("none".to_string());
    let (grid_layout, set_grid_layout_sig) = signal("grid".to_string());
    let (room_config, set_room_config) = signal(shared::RoomConfig::default());
    let is_audio_moderated = Signal::derive(move || room_config.get().audio_moderation_enabled);
    let is_video_moderated = Signal::derive(move || room_config.get().video_moderation_enabled);
    let (branding, set_branding_sig) = signal(shared::BrandingConfig::default());
    let (lobby_announcement, set_lobby_announcement) = signal(None::<String>);
    let (dominant_speaker, set_dominant_speaker) = signal(None::<String>);
    let (_last_face_expression, set_face_expression) = signal(None::<(String, String, u64)>);
    let is_face_landmarks_enabled = RwSignal::new(false);
    let (is_audio_only, set_is_audio_only) = signal(false);
    let (is_flipped, set_is_flipped) = signal(false);
    let (pinned_participant, set_pinned_participant) = signal(None::<String>);
    let (participant_volumes, set_participant_volumes) = signal(HashMap::<String, f64>::new());

    let (remote_streams, set_remote_streams) = signal(HashMap::<String, Vec<MediaStream>>::new());

    let (power_statuses, set_power_statuses) =
        signal(std::collections::HashMap::<String, shared::PowerStatus>::new());
    let (is_recording_locally, set_is_recording_locally) = signal(false);
    let (is_talking_while_muted, set_is_talking_while_muted) = signal(false);
    let (show_rejoin, set_show_rejoin) = signal(false);
    let local_recorder: SendWrapper<Rc<RefCell<Option<crate::media_recorder::LocalRecorder>>>> =
        SendWrapper::new(Rc::new(RefCell::new(None)));
    // Holds previously-stopped recorders whose async `onstop` callbacks may
    // not have fired yet. They are kept alive here so the wasm-bindgen
    // Closures remain valid until the browser event loop processes the stop
    // event. Entries are cleared each time a new recording starts.
    let pending_recorders: SendWrapper<Rc<RefCell<Vec<crate::media_recorder::LocalRecorder>>>> =
        SendWrapper::new(Rc::new(RefCell::new(Vec::new())));
    // Tracks the stream ID that the active LocalRecorder was created with.
    // Used by a reactive effect to detect when `local_stream` is replaced
    // (e.g. camera toggle, device switch) and automatically restart the
    // recording on the new stream.
    let recording_stream_id: SendWrapper<Rc<RefCell<Option<String>>>> =
        SendWrapper::new(Rc::new(RefCell::new(None)));

    // Video Processing for Virtual Background
    let (raw_local_stream, set_raw_local_stream) = signal(None::<MediaStream>);

    // Reactive signals that need to be defined before callbacks
    let host_id = Signal::derive(move || room_config.get().host_id);

    let is_host = Signal::derive(move || {
        let h = host_id.get();
        let my = my_id.get();

        match (h, my) {
            (Some(host), Some(me)) => host == me,
            _ => false,
        }
    });

    let is_visitor = Signal::derive(move || {
        if let Some(me_id) = my_id.get() {
            participants
                .get()
                .iter()
                .find(|p| p.id == me_id)
                .map(|p| p.is_visitor)
                .unwrap_or(false)
        } else {
            false
        }
    });

    // Note: Reactive Noise Suppression effect is created after start_media_stream below.

    // Track the stream ID so we can detect when only the mode changed (same
    // stream) vs when the underlying raw stream was replaced (camera toggle,
    // device switch, noise-suppression restart).
    let prev_stream_id: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    Effect::new({
        let prev_stream_id = prev_stream_id.clone();
        move |prev_processor: Option<Option<crate::media::VideoProcessor>>| -> Option<crate::media::VideoProcessor> {
            let mode = background_mode.get();
            let stream = raw_local_stream.get();

            if let Some(s) = stream {
                let has_video = s.get_video_tracks().length() > 0;
                if mode == "none" || !has_video {
                    // No processing needed — drop any existing processor and pass through.
                    // Stop old canvas stream video tracks to avoid leaking captureStream
                    // resources. Only stop video tracks since audio tracks are shared
                    // references to the raw stream's tracks and must remain active.
                    if prev_processor.as_ref().is_some_and(|p| p.is_some())
                        && let Some(old_processed) = local_stream.get_untracked() {
                            let video_tracks = old_processed.get_video_tracks();
                            for i in 0..video_tracks.length() {
                                if let Ok(track) = video_tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                                    track.stop();
                                }
                            }
                        }
                    if let Some(Some(prev)) = prev_processor {
                        drop(prev);
                    }
                    *prev_stream_id.borrow_mut() = Some(s.id());
                    set_local_stream.set(Some(s));
                    None
                } else {
                    // Check whether the raw stream changed since the last run.
                    let stream_changed = {
                        let old_id = prev_stream_id.borrow();
                        old_id.as_deref() != Some(&s.id())
                    };
                    *prev_stream_id.borrow_mut() = Some(s.id());

                    if !stream_changed
                        && let Some(Some(prev)) = prev_processor {
                            // Same stream, only mode changed — update in-place instead of
                            // recreating the canvas, video element, interval, and captureStream.
                            prev.set_mode(mode);
                            return Some(prev);
                        }

                    // Either the stream changed or no processor exists yet — (re)create.
                    // Stop old canvas video tracks before dropping the processor.
                    if prev_processor.as_ref().is_some_and(|p| p.is_some())
                        && let Some(old_processed) = local_stream.get_untracked() {
                            let video_tracks = old_processed.get_video_tracks();
                            for i in 0..video_tracks.length() {
                                if let Ok(track) = video_tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                                    track.stop();
                                }
                            }
                        }
                    if let Some(Some(prev)) = prev_processor {
                        drop(prev);
                    }
                    match crate::media::VideoProcessor::new(&s, mode) {
                        Ok((processor, processed)) => {
                            set_local_stream.set(Some(processed));
                            Some(processor)
                        }
                        Err(e) => {
                            web_sys::console::error_1(&e);
                            set_local_stream.set(Some(s));
                            None
                        }
                    }
                }
            } else {
                // No stream — drop any existing processor
                if let Some(Some(prev)) = prev_processor {
                    drop(prev);
                }
                *prev_stream_id.borrow_mut() = None;
                set_local_stream.set(None);
                None
            }
        }
    });

    let set_grid_layout = Callback::new(move |layout: String| {
        set_grid_layout_sig.set(layout.clone());
        if is_host.get_untracked()
            && let Some(socket) = ws.get()
        {
            let msg = ClientMessage::FollowMe(layout);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let close_poll = Callback::new(move |poll_id: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::ClosePoll(poll_id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    // WebRTC Manager Setup
    let ws_clone_for_webrtc = ws;
    let send_signal_cb = move |msg: ClientMessage| {
        if let Some(socket) = ws_clone_for_webrtc.get_untracked()
            && let Ok(json) = serde_json::to_string(&msg)
        {
            let _ = socket.send_with_str(&json);
        }
    };

    // Provide Analytics context
    provide_analytics_context(Callback::new(send_signal_cb));
    let analytics = use_analytics();
    provide_face_landmarks_context(Callback::new(send_signal_cb));
    let face_landmarks = use_face_landmarks();
    provide_remote_control_context(Callback::new(send_signal_cb));
    let remote_control = use_remote_control();
    provide_salesforce_context(Callback::new(send_signal_cb));
    provide_dropbox_context(Callback::new(send_signal_cb));

    // Periodic Analytics: Track performance stats (RTT, Audio Level) every 10 seconds
    Effect::new({
        let analytics = analytics.clone();
        move |_| {
            let analytics_inner = analytics.clone();
            let handle = gloo_timers::callback::Interval::new(10000, move || {
                #[cfg(target_arch = "wasm32")]
                {
                    let rtt_val = rtt.get_untracked();
                    let level_val = audio_level.get_untracked();
                    let props = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(
                        &props,
                        &wasm_bindgen::JsValue::from_str("rtt"),
                        &wasm_bindgen::JsValue::from_f64(rtt_val as f64),
                    );
                    let _ = js_sys::Reflect::set(
                        &props,
                        &wasm_bindgen::JsValue::from_str("audio_level"),
                        &wasm_bindgen::JsValue::from_f64(level_val),
                    );
                    analytics_inner.track_event("perf_stats", props.into());
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = analytics_inner;
                }
            });
            on_cleanup_local(move || drop(handle));
        }
    });

    Effect::new({
        let face_landmarks = face_landmarks.clone();
        move |_| {
            if is_face_landmarks_enabled.get() {
                face_landmarks.start();
            } else {
                face_landmarks.stop();
            }
        }
    });

    let on_track_cb_clone = set_remote_streams;
    let on_track_cb = move |peer_id: String, stream: MediaStream| {
        // Add cleanup listener for tracks
        let tracks = stream.get_tracks();
        for i in 0..tracks.length() {
            if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                let peer_id_clone = peer_id.clone();
                let stream_id = stream.id();
                let set_remote_streams = on_track_cb_clone; // captured

                let onended = Closure::wrap(Box::new(move || {
                    set_remote_streams.update(|map: &mut HashMap<String, Vec<MediaStream>>| {
                        if let Some(streams) = map.get_mut(&peer_id_clone) {
                            streams.retain(|s| {
                                // If the stream matches the one ending, check if it's still active
                                if s.id() == stream_id {
                                    s.active()
                                } else {
                                    true
                                }
                            });
                        }
                    });
                }) as Box<dyn FnMut()>);
                track.set_onended(Some(onended.as_ref().unchecked_ref()));
                onended.forget();
            }
        }

        set_remote_streams.update(|map: &mut HashMap<String, Vec<MediaStream>>| {
            // Append the new stream to the list for this peer, checking for duplicates
            let streams = map.entry(peer_id).or_default();
            let stream_id = stream.id();
            if !streams.iter().any(|s| s.id() == stream_id) {
                streams.push(stream);
            }
        });
    };

    let webrtc_manager = WebRTCManager::new(
        send_signal_cb,
        on_track_cb,
        local_stream.into(),
        local_screen_stream.into(),
        my_id.into(),
    );

    // Dynamic Stream Handling: Initiate connections when local stream becomes available
    let participants_for_effect = participants;
    let my_id_for_effect = my_id;
    let webrtc_manager_clone = webrtc_manager.clone();

    Effect::new(move |_| {
        // Run when my_id or local_stream changes
        let my_id_val = my_id_for_effect.get();
        // We track local_stream and local_screen_stream to trigger track updates
        let _ = local_stream.get();
        let _ = local_screen_stream.get();

        if let Some(me) = my_id_val {
            // Update tracks for existing connections (e.g. from incoming offers)
            // This is safe to call even if local_stream is None (it effectively clears tracks or does nothing)
            webrtc_manager_clone.update_local_tracks();

            let list = participants_for_effect.get_untracked();
            for p in list {
                if p.id != me {
                    // Only initiate connection if one doesn't exist AND I am the impolite peer (higher ID)
                    // Deterministic rule: Higher ID initiates.
                    if !webrtc_manager_clone.has_peer(&p.id) && me > p.id {
                        webrtc_manager_clone.handle_participant_joined(p.id);
                    }
                }
            }
        }
    });

    // Internal state to trigger media start after joining
    let (start_media_on_join, set_start_media_on_join) = signal(false);
    let (initial_cam_on, set_initial_cam_on) = signal(false);

    // Setup listener for "talk while muted"
    let toast_context = crate::components_ui::toast::use_toast();
    Effect::new(move |_| {
        use wasm_bindgen::JsCast;
        if let Some(window) = web_sys::window() {
            let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
                set_is_talking_while_muted.set(true);
                toast_context.add(
                    "You are muted. Please unmute to speak.".to_string(),
                    crate::components_ui::toast::ToastType::Error,
                );
                set_timeout(
                    move || {
                        set_is_talking_while_muted.set(false);
                    },
                    std::time::Duration::from_secs(3),
                );
            }) as Box<dyn FnMut(_)>);

            let _ = window.add_event_listener_with_callback(
                "talk_while_muted",
                closure.as_ref().unchecked_ref(),
            );

            // Clean up the event listener when the component is unmounted
            on_cleanup_local(move || {
                if let Some(win) = web_sys::window() {
                    let _ = win.remove_event_listener_with_callback(
                        "talk_while_muted",
                        closure.as_ref().unchecked_ref(),
                    );
                }
            });
        }
    });

    Effect::new({
        let toast_ctx_inner = toast_ctx;
        move |_| {
            if let Some(window) = web_sys::window() {
                let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
                    toast_ctx_inner.add_advanced(
                        "High background noise detected. Consider enabling Noise Suppression in Settings.".to_string(),
                        ToastType::Warning,
                        false,
                        1,
                    );
                }) as Box<dyn FnMut(_)>);

                let _ = window.add_event_listener_with_callback(
                    "noise_detected",
                    closure.as_ref().unchecked_ref(),
                );

                on_cleanup_local(move || {
                    if let Some(win) = web_sys::window() {
                        let _ = win.remove_event_listener_with_callback(
                            "noise_detected",
                            closure.as_ref().unchecked_ref(),
                        );
                    }
                });
            }
        }
    });

    let add_toast = move |msg: String, type_: ToastType| {
        toast_ctx.add(msg, type_);
    };

    // Extract start_media_stream logic
    let start_media_stream = Callback::new(move |enable_video: bool| {
        // Clear host-muted camera state since we are replacing the stream
        set_is_camera_off.set(false);
        spawn_local(async move {
            let v_id = selected_camera_id.get_untracked();
            let a_id = selected_mic_id.get_untracked();
            let res = video_resolution.get_untracked();

            if let Ok(stream) = get_user_media(enable_video, true, v_id, a_id, Some(&res)).await {
                // Stop existing raw stream tracks to release camera/mic hardware.
                if let Some(old_raw) = raw_local_stream.get_untracked() {
                    let tracks = old_raw.get_tracks();
                    for i in 0..tracks.length() {
                        if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                            track.stop();
                        }
                    }
                }
                // Also stop processed stream tracks (canvas video tracks) for cleanup
                if let Some(old_stream) = local_stream.get_untracked() {
                    let tracks = old_stream.get_tracks();
                    for i in 0..tracks.length() {
                        if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                            track.stop();
                        }
                    }
                }
                set_audio_monitor.set(None); // Reset monitor for new stream

                // Apply existing mute state to new stream
                if is_muted.get_untracked() {
                    let audio_tracks = stream.get_audio_tracks();
                    for i in 0..audio_tracks.length() {
                        if let Ok(track) =
                            audio_tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>()
                        {
                            track.set_enabled(false);
                        }
                    }
                }

                set_raw_local_stream.set(Some(stream.clone()));

                let on_speaking = Box::new(move |is_speaking: bool| {
                    if let Some(socket) = ws.get_untracked() {
                        let msg = ClientMessage::Speaking(is_speaking);
                        if let Ok(json) = serde_json::to_string(&msg) {
                            let _ = socket.send_with_str(&json);
                        }
                    }
                });

                let add_toast_clone = add_toast;
                let on_no_audio = Box::new(move || {
                    add_toast_clone(
                        "No audio input detected. Please check your microphone.".to_string(),
                        crate::components_ui::toast::ToastType::Error,
                    );
                });

                // Only push a new value when it meaningfully differs from the
                // previous one. The AudioMonitor fires this callback every
                // 100ms; most ticks while idle/muted report 0.0 and an
                // unconditional `set` would trigger ~10 reactive updates
                // per second for no visible change.
                let on_level = Box::new(move |level: f64| {
                    let prev = audio_level.get_untracked();
                    if (level == 0.0 && prev != 0.0)
                        || (level != 0.0 && (prev - level).abs() > 0.01)
                    {
                        set_audio_level.set(level);
                    }
                });

                if let Ok(monitor) = AudioMonitor::new(
                    &stream,
                    on_speaking,
                    Some(on_level as Box<dyn FnMut(f64)>),
                    Some(on_no_audio as Box<dyn FnMut()>),
                    is_noise_suppression_enabled.get_untracked(),
                ) {
                    // Inherit the current mute state so the monitor doesn't fire
                    // false-positive speaking callbacks while the user is muted
                    // (e.g. after a noise-suppression restart or device change).
                    monitor.set_muted(is_muted.get_untracked());
                    set_audio_monitor.set(Some(monitor));
                }
            }
        });
    });

    // Dominant Speaker Update: pick the remote peer who has been
    // continuously speaking the longest in the current session. The
    // server only reports binary speaking/not-speaking via `PeerSpeaking`
    // (no volume), so we approximate "most active" by tracking when each
    // peer's current speaking session began and picking the earliest.
    //
    // This produces a naturally sticky spotlight: once a peer becomes
    // dominant they stay dominant as long as they keep speaking, even
    // when other peers briefly join in. Lexicographic ordering by id is
    // used only as a final tiebreaker for peers that started in the same
    // tick.
    //
    // When no one is currently speaking, we retain the previous dominant
    // speaker (as long as they are still in the room) so the spotlight
    // doesn't visually shift to an arbitrary participant during natural
    // pauses in conversation. The signal is only cleared when the last
    // dominant speaker leaves the room.
    let speaker_starts: Rc<RefCell<HashMap<String, f64>>> = Rc::new(RefCell::new(HashMap::new()));
    Effect::new(move |_| {
        let peers = speaking_peers.get();
        let me = my_id.get();
        // Track `participants` reactively so the effect re-runs when a
        // participant leaves the room. Without this, a stale dominant
        // speaker ID would persist after the speaker leaves while nobody
        // else is currently speaking, leaving spotlight mode unable to
        // resolve the featured tile (VideoGrid's `or_else` fallback only
        // runs when `dominant_speaker` is `None`).
        let participants_list = participants.get();
        let now = js_sys::Date::now();

        let mut starts = speaker_starts.borrow_mut();
        // Record the start time for any new speakers in this tick.
        for p in peers.iter() {
            starts.entry(p.clone()).or_insert(now);
        }
        // Drop entries for peers who are no longer speaking so a future
        // speaking session is treated as a fresh start.
        starts.retain(|k, _| peers.contains(k));

        // Pick the remote peer with the earliest start time. `f64` is not
        // Ord, so compare manually with NaN-safe ordering. Falls back to
        // id ordering when start times are equal.
        let speaker = peers
            .iter()
            .filter(|id| me.as_ref() != Some(*id))
            .min_by(|a, b| {
                let ta = starts.get(*a).copied().unwrap_or(f64::INFINITY);
                let tb = starts.get(*b).copied().unwrap_or(f64::INFINITY);
                ta.partial_cmp(&tb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.cmp(b))
            })
            .cloned();

        match speaker {
            // A remote peer is currently speaking — they become the
            // dominant speaker.
            Some(s) => set_dominant_speaker.set(Some(s)),
            // No remote peer is speaking. Keep the previous dominant
            // speaker featured if they are still in the room; otherwise
            // clear the signal so VideoGrid's fallback can take over.
            None => {
                let prev = dominant_speaker.get_untracked();
                if let Some(prev_id) = prev {
                    let still_present = participants_list.iter().any(|p| p.id == prev_id);
                    if !still_present {
                        set_dominant_speaker.set(None);
                    }
                    // else: leave the existing value untouched (sticky).
                }
            }
        }
    });

    // Reactive Noise Suppression Update
    Effect::new(move |_| {
        let enabled = is_noise_suppression_enabled.get();
        let needs_restart = audio_monitor.with_untracked(|monitor| {
            if let Some(m) = monitor {
                // Returns Err if the monitor needs to be recreated (e.g. enabling
                // suppression when no compressor node exists in the audio graph).
                m.set_noise_suppression(enabled).is_err()
            } else {
                false
            }
        });
        if needs_restart {
            // Restart media to recreate the AudioMonitor with the correct setting.
            // Preserve current video state.
            let has_video = local_stream.with_untracked(|s| {
                s.as_ref()
                    .is_some_and(|stream| stream.get_video_tracks().length() > 0)
            });
            if local_stream.get_untracked().is_some() {
                start_media_stream.run(has_video);
            }
        }
    });

    // Initialize WebSocket
    Effect::new({
        let analytics_for_ws = analytics.clone();
        let local_recorder_for_cleanup = local_recorder.clone();
        let pending_recorders_for_cleanup = pending_recorders.clone();
        let recording_stream_id_for_cleanup = recording_stream_id.clone();
        let webrtc_manager_for_ws = webrtc_manager.clone();
        let remote_control = remote_control.clone();
        move |_| {
            let analytics = analytics_for_ws.clone();
            let remote_control_for_effect = remote_control.clone();
            let local_recorder_for_cleanup = local_recorder_for_cleanup.clone();
            let pending_recorders_for_cleanup = pending_recorders_for_cleanup.clone();
            let recording_stream_id_for_cleanup = recording_stream_id_for_cleanup.clone();
            set_my_id.set(None);
            set_room_config.set(shared::RoomConfig::default());

            let location = web_sys::window().unwrap().location();
            let protocol = if location.protocol().unwrap() == "https:" {
                "wss:"
            } else {
                "ws:"
            };
            let host = location.host().unwrap();
            let url = format!("{}//{}/ws/chat", protocol, host);

            if let Ok(socket) = WebSocket::new(&url) {
                let webrtc_manager = webrtc_manager_for_ws.clone();
                let remote_control_clone = remote_control_for_effect.clone();
                let onmessage_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
                    if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                        let txt: String = txt.into();
                        if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&txt) {
                            let ctx = HandlerContext {
                                set_my_id,
                                set_current_state,
                                analytics: analytics.clone(),
                                start_media_on_join,
                                initial_cam_on,
                                start_media_stream,
                                set_start_media_on_join,
                                is_muted,
                                ws,
                                local_stream,
                                raw_local_stream,
                                add_toast: Callback::new(move |(msg, t)| add_toast(msg, t)),
                                set_is_camera_off,
                                room_config,
                                set_show_etherpad,
                                set_is_locked,
                                set_password_required,
                                set_is_e2ee_enabled,
                                is_recording,
                                set_is_recording,
                                set_is_lobby_enabled,
                                is_subtitles_enabled,
                                set_is_subtitles_enabled,
                                set_subtitles,
                                set_room_config,
                                current_room_id,
                                set_messages,
                                set_is_connected,
                                set_knocking_participants,
                                set_participants,
                                my_id,
                                webrtc_manager: webrtc_manager.clone(),
                                set_typing_users,
                                set_speaking_peers,
                                set_power_statuses,
                                set_remote_streams,
                                is_recording_locally,
                                local_recorder: local_recorder_for_cleanup.clone(),
                                pending_recorders: pending_recorders_for_cleanup.clone(),
                                recording_stream_id: recording_stream_id_for_cleanup.clone(),
                                set_is_recording_locally,
                                set_is_muted,
                                set_audio_monitor,
                                set_branding_sig,
                                local_screen_stream,
                                set_local_screen_stream,
                                participants,
                                set_last_reaction,
                                set_breakout_rooms,
                                set_polls,
                                set_grid_layout_sig,
                                set_whiteboard_history,
                                set_last_draw_action,
                                set_whiteboard_reset,
                                set_shared_video_url,
                                last_ping_time,
                                set_rtt,
                                set_is_authenticated,
                                set_show_login_dialog,
                                set_auth_error,
                                set_calendar_events,
                                set_lobby_announcement,
                                set_face_expression,
                                set_current_room_id,
                                set_is_audio_only,
                                set_is_flipped,
                                set_pinned_participant,
                                set_participant_volumes,
                                set_pending_unmute_requests,
                                set_pending_camera_requests,
                                set_has_unmute_permission,
                                set_has_camera_permission,
                                remote_control: remote_control_clone.clone(),
                            };
                            handle_server_message(server_msg, &ctx);
                        }
                    }
                });
                socket.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
                onmessage_callback.forget();

                let onopen_callback = Closure::<dyn FnMut()>::new(move || {
                    set_is_connected.set(true);
                });
                socket.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
                onopen_callback.forget();

                let onclose_callback = Closure::<dyn FnMut()>::new(move || {
                    set_is_connected.set(false);
                    if current_state.get_untracked() == RoomConnectionState::Joined {
                        set_show_rejoin.set(true);
                    }
                });
                socket.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
                onclose_callback.forget();

                let onerror_callback = Closure::<dyn FnMut()>::new(move || {
                    set_is_connected.set(false);
                    if current_state.get_untracked() == RoomConnectionState::Joined {
                        set_show_rejoin.set(true);
                    }
                });
                socket.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
                onerror_callback.forget();

                set_ws.set(Some(socket));
            }
        }
    });

    let request_unmute_permission = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::RequestUnmutePermission;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let request_camera_permission = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::RequestCameraPermission;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let grant_unmute_permission = Callback::new(move |target_id: String| {
        set_pending_unmute_requests.update(|set| {
            set.remove(&target_id);
        });
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::GrantUnmutePermission(target_id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let grant_camera_permission = Callback::new(move |target_id: String| {
        set_pending_camera_requests.update(|set| {
            set.remove(&target_id);
        });
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::GrantCameraPermission(target_id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let toggle_audio_moderation = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::ToggleAudioModeration;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let toggle_video_moderation = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::ToggleVideoModeration;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });
    let webrtc_manager_cleanup = webrtc_manager.clone();
    let _remote_control_cleanup = remote_control.clone();
    on_cleanup_local(move || {
        if let Some(socket) = ws.get() {
            let _ = socket.close();
        }
        webrtc_manager_cleanup.close_all_peers();
    });

    let send_message = Callback::new(
        move |(content, recipient_id, attachment, room_id): (
            String,
            Option<String>,
            Option<FileAttachment>,
            Option<String>,
        )| {
            if let Some(socket) = ws.get() {
                let msg = ClientMessage::Chat {
                    content,
                    recipient_id,
                    attachment,
                    room_id,
                };
                if let Ok(json) = serde_json::to_string(&msg) {
                    let _ = socket.send_with_str(&json);
                }
            }
        },
    );

    let analytics_for_lock = analytics.clone();
    let toggle_lock = Callback::new(move |password: Option<String>| {
        analytics_for_lock.track_interaction("toggle_lock");
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::ToggleRoomLock(password);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let toggle_e2ee = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::ToggleE2EE;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let toggle_participant_e2ee = Callback::new({
        let analytics = analytics.clone();
        move |enabled: bool| {
            analytics.track_interaction(if enabled {
                "enable_e2ee"
            } else {
                "disable_e2ee"
            });
            // Tell the server our per-participant E2EE preference so other
            // participants see the lock indicator on our tile.
            if let Some(socket) = ws.get() {
                let msg = ClientMessage::UpdateE2EE(enabled);
                if let Ok(json) = serde_json::to_string(&msg) {
                    let _ = socket.send_with_str(&json);
                }
            }
            if enabled {
                // Generate a mock key for this participant
                let key = js_sys::Math::random().to_string();
                set_e2ee_key.set(Some(key.clone()));
                if let Some(socket) = ws.get() {
                    let msg = ClientMessage::E2EEKeyExchange(key);
                    if let Ok(json) = serde_json::to_string(&msg) {
                        let _ = socket.send_with_str(&json);
                    }
                }
            } else {
                set_e2ee_key.set(None);
            }
        }
    });

    let toggle_etherpad = Callback::new(move |url: Option<String>| {
        if let Some(url_str) = &url
            && !url_str.starts_with("https://")
            && !url_str.starts_with("http://")
        {
            add_toast("Invalid Etherpad URL".to_string(), ToastType::Error);
            return;
        }

        if let Some(socket) = ws.get() {
            let msg = ClientMessage::SetEtherpadUrl(url);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let toggle_lobby = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::ToggleLobby;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let toggle_subtitles = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::ToggleSubtitles;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let set_presence = Callback::new(move |status: shared::PresenceStatus| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::SetPresence(status);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let set_background_mode = Callback::new(move |mode: String| {
        set_background_mode_sig.set(mode);
    });

    let grant_access = Callback::new(move |id: String| {
        set_knocking_participants.update(|list: &mut Vec<Participant>| list.retain(|p| p.id != id));
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::GrantAccess(id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let deny_access = Callback::new(move |id: String| {
        set_knocking_participants.update(|list: &mut Vec<Participant>| list.retain(|p| p.id != id));
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::DenyAccess(id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let toggle_recording = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::ToggleRecording;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let save_profile = Callback::new(move |new_name: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::UpdateProfile(new_name);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let set_avatar_url = Callback::new(move |url: Option<String>| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::UpdateAvatar(url);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let set_subject = Callback::new(move |subject: String| {
        if let Some(socket) = ws.get() {
            // Normalize empty strings to `None` so clearing the subject stores
            // `None` rather than `Some("")` in the room config.
            let payload = if subject.is_empty() {
                None
            } else {
                Some(subject)
            };
            let msg = ClientMessage::SetSubject(payload);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let toggle_audio_only = Callback::new(move |enabled: bool| {
        set_is_audio_only.set(enabled);
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::SetAudioOnly(enabled);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let toggle_flip = Callback::new(move |enabled: bool| {
        set_is_flipped.set(enabled);
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::FlipLocalVideo(enabled);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let pin_participant = Callback::new(move |target_id: Option<String>| {
        set_pinned_participant.set(target_id.clone());
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::PinParticipant(target_id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let set_participant_volume = Callback::new(move |(target_id, volume): (String, f64)| {
        set_participant_volumes.update(|map| {
            map.insert(target_id.clone(), volume);
        });
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::SetParticipantVolume { target_id, volume };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let mute_everyone_else = Callback::new(move |target_id: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::MuteEveryoneElse(target_id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let analytics_for_reaction = analytics.clone();
    let send_reaction = Callback::new(move |emoji: String| {
        if is_visitor.get_untracked() {
            return;
        }
        let props = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &props,
            &JsValue::from_str("emoji"),
            &JsValue::from_str(&emoji),
        );
        analytics_for_reaction.track_event("send_reaction", props.into());
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::Reaction(emoji);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let analytics_for_raise_hand = analytics.clone();
    let toggle_raise_hand = Callback::new(move |_: ()| {
        if is_visitor.get_untracked() {
            return;
        }
        analytics_for_raise_hand.track_interaction("toggle_raise_hand");
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::ToggleRaiseHand;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let webrtc_manager_for_screen = webrtc_manager.clone();
    let toggle_screen_share = Callback::new(move |_: ()| {
        if is_visitor.get_untracked() {
            return;
        }
        if local_screen_stream.get().is_some() {
            // Stop sharing
            if let Some(stream) = local_screen_stream.get() {
                let tracks = stream.get_tracks();
                for i in 0..tracks.length() {
                    if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                        track.stop();
                    }
                }
            }
            set_local_screen_stream.set(None);
            webrtc_manager_for_screen.update_local_tracks();

            // Notify server stopped
            if let Some(socket) = ws.get() {
                let msg = ClientMessage::ToggleScreenShare;
                if let Ok(json) = serde_json::to_string(&msg) {
                    let _ = socket.send_with_str(&json);
                }
            }
        } else {
            // Start sharing
            let webrtc_manager_for_spawn = webrtc_manager_for_screen.clone();
            spawn_local(async move {
                match get_display_media().await {
                    Ok(stream) => {
                        set_local_screen_stream.set(Some(stream));
                        webrtc_manager_for_spawn.update_local_tracks();

                        // Notify server started
                        if let Some(socket) = ws.get() {
                            let msg = ClientMessage::ToggleScreenShare;
                            if let Ok(json) = serde_json::to_string(&msg) {
                                let _ = socket.send_with_str(&json);
                            }
                        }
                    }
                    Err(e) => {
                        web_sys::console::error_1(&e);
                    }
                }
            });
        }
    });

    let analytics_for_create_poll = analytics.clone();
    let create_poll = Callback::new(move |poll: Poll| {
        analytics_for_create_poll.track_interaction("create_poll");
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::CreatePoll(poll);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let analytics_for_vote_poll = analytics.clone();
    let vote_poll = Callback::new(move |(poll_id, option_id): (String, u32)| {
        analytics_for_vote_poll.track_interaction("vote_poll");
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::Vote { poll_id, option_id };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let send_draw = Callback::new(move |action: DrawAction| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::Draw(action);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let join_meeting = Callback::new(move |options: JoinOptions| {
        // Persist settings
        let display_name_clone = options.display_name.clone();
        let cam_id_clone = options.video_device_id.clone();
        let mic_id_clone = options.audio_device_id.clone();

        update_setting(move |s| {
            s.display_name = Some(display_name_clone);
            s.camera_id = cam_id_clone;
            s.mic_id = mic_id_clone;
        });

        // Note: the meeting room is added to the recent rooms list only
        // after the server confirms the join via `ServerMessage::Welcome`,
        // so rooms that reject the join (locked, full, etc.) do not end
        // up in the user's recent rooms. See `state_handlers.rs`.

        // Set initial state
        set_is_muted.set(!options.mic_enabled);
        set_selected_mic_id.set(options.audio_device_id);
        set_selected_camera_id.set(options.video_device_id);

        // Start media if either mic or cam is on
        set_start_media_on_join.set(options.mic_enabled || options.camera_enabled);
        set_initial_cam_on.set(options.camera_enabled);

        let display_name = options.display_name;
        let is_visitor = options.is_visitor;
        let avatar_url = options.avatar_url;
        let password = options.password;

        if let Some(socket) = ws.get() {
            let msg = ClientMessage::Join {
                name: display_name,
                is_visitor,
                avatar_url,
                password,
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let set_is_typing = Callback::new(move |is_typing: bool| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::Typing(is_typing);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let rejoin = Callback::new(move |_| {
        if let Some(window) = web_sys::window() {
            let _ = window.location().reload();
        }
    });

    let create_breakout_room = Callback::new(move |name: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::CreateBreakoutRoom(name);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let remove_breakout_room = Callback::new(move |room_id: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::RemoveBreakoutRoom(room_id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let rename_breakout_room = Callback::new(move |(room_id, new_name): (String, String)| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::RenameBreakoutRoom { room_id, new_name };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let close_all_breakout_rooms = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::CloseAllBreakoutRooms;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let move_participant_to_room =
        Callback::new(move |(target_id, room_id): (String, Option<String>)| {
            if let Some(socket) = ws.get() {
                let msg = ClientMessage::MoveParticipantToRoom { target_id, room_id };
                if let Ok(json) = serde_json::to_string(&msg) {
                    let _ = socket.send_with_str(&json);
                }
            }
        });

    let auto_assign_to_breakout_rooms = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::AutoAssignToBreakoutRooms;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let webrtc_manager_for_breakout = webrtc_manager.clone();
    let join_breakout_room = Callback::new(move |room_id: Option<String>| {
        set_current_room_id.set(room_id.clone());
        // Clear messages when switching rooms
        set_messages.set(Vec::new());
        // Clear stale indicators
        set_speaking_peers.update(|s: &mut HashSet<String>| s.clear());
        set_typing_users.update(|u: &mut HashSet<String>| u.clear());
        // Clear stale power statuses from the previous room; each
        // participant's PowerMonitor will re-send their status on the
        // next 60-second poll cycle.
        set_power_statuses.set(std::collections::HashMap::new());

        // Cleanup existing WebRTC connections on room switch
        webrtc_manager_for_breakout.close_all_peers();
        set_remote_streams.set(HashMap::new());

        if let Some(socket) = ws.get() {
            let msg = ClientMessage::JoinBreakoutRoom(room_id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let kick_participant = Callback::new(move |id: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::KickParticipant(id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let mute_participant = Callback::new(move |id: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::MuteParticipant(id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let mute_camera_participant = Callback::new(move |id: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::MuteCameraParticipant(id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let mute_all = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::MuteAll;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let mute_camera_all = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::MuteCameraAll;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let stop_screen_share_all = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::StopScreenShareAll;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let set_branding = Callback::new(move |b: shared::BrandingConfig| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::SetBranding(b);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let transfer_host = Callback::new(move |id: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::TransferHost(id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let end_meeting = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::EndMeeting;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let toggle_local_recording = Callback::new({
        let local_recorder = local_recorder.clone();
        let pending_recorders = pending_recorders.clone();
        let recording_stream_id = recording_stream_id.clone();
        move |explicit_state: bool| {
            let is_active = is_recording_locally.get_untracked();
            if is_active == explicit_state {
                return;
            }
            if is_active {
                // Call stop() but keep the LocalRecorder alive so its Closure
                // callbacks (_on_data_available, _on_stop) remain valid when
                // the browser asynchronously fires the stop event. Move the
                // recorder into `pending_recorders`; it will be dropped the
                // next time the user starts a new recording.
                if let Some(r) = local_recorder.borrow_mut().take() {
                    r.stop();
                    pending_recorders.borrow_mut().push(r);
                }
                *recording_stream_id.borrow_mut() = None;
                set_is_recording_locally.set(false);
                if let Some(socket) = ws.get() {
                    let _ = socket.send_with_str(
                        &serde_json::to_string(&ClientMessage::ToggleLocalRecording(false))
                            .unwrap(),
                    );
                }
            } else if let Some(stream) = local_stream.get_untracked() {
                // Drop any previously-stopped recorders. By now their async
                // `onstop` callbacks have had ample time to fire (the user
                // had to click stop, wait, then click start again).
                pending_recorders.borrow_mut().clear();
                match crate::media_recorder::LocalRecorder::new(
                    stream.clone(),
                    Callback::new(move |msg: String| {
                        add_toast(format!("Recording error: {}", msg), ToastType::Error);
                    }),
                ) {
                    Ok(r) => {
                        *local_recorder.borrow_mut() = Some(r);
                        *recording_stream_id.borrow_mut() = Some(stream.id());
                        set_is_recording_locally.set(true);
                        if let Some(socket) = ws.get() {
                            let _ = socket.send_with_str(
                                &serde_json::to_string(&ClientMessage::ToggleLocalRecording(true))
                                    .unwrap(),
                            );
                        }
                    }
                    Err(e) => {
                        web_sys::console::error_1(&e);
                        add_toast(
                            "Failed to start local recording".to_string(),
                            ToastType::Error,
                        );
                    }
                }
            } else {
                add_toast(
                    "Enable camera or microphone first to start recording".to_string(),
                    ToastType::Error,
                );
            }
        }
    });

    // Reactive effect: when local_stream changes while recording, automatically
    // stop the old recorder and start a new one on the fresh stream. This handles
    // camera toggles, device switches, and noise-suppression restarts that replace
    // the underlying MediaStream whose tracks the recorder depends on.
    {
        let local_recorder = local_recorder.clone();
        let pending_recorders = pending_recorders.clone();
        let recording_stream_id = recording_stream_id.clone();
        Effect::new(move |_| {
            let current_stream = local_stream.get();
            let is_active = is_recording_locally.get_untracked();
            if !is_active {
                return;
            }

            let old_id = recording_stream_id.borrow().clone();
            let new_id = current_stream.as_ref().map(|s| s.id());

            // Only act when the stream identity actually changed
            if old_id == new_id {
                return;
            }

            // Stop the old recorder, keeping it alive for async onstop
            if let Some(r) = local_recorder.borrow_mut().take() {
                r.stop();
                pending_recorders.borrow_mut().push(r);
            }

            // Trim very old pending recorders to prevent unbounded growth
            // during rapid stream changes. Keep the most recent entries
            // whose async onstop callbacks may not have fired yet; older
            // ones have had enough time (each stream change is user- or
            // system-initiated with perceptible delay).
            {
                let mut pending = pending_recorders.borrow_mut();
                if pending.len() > 3 {
                    let drain_count = pending.len() - 3;
                    pending.drain(..drain_count);
                }
            }

            if let Some(stream) = current_stream {
                // Start a new recorder on the replacement stream.
                // Do NOT clear pending_recorders here — the old recorder
                // was just pushed above and its async onstop callback has
                // not fired yet. Clearing now would drop the Closure and
                // lose the recorded data. Stale recorders are cleaned up
                // the next time the user manually starts a new recording
                // (in toggle_local_recording) after an interactive delay.
                match crate::media_recorder::LocalRecorder::new(
                    stream.clone(),
                    Callback::new(move |msg: String| {
                        add_toast(format!("Recording error: {}", msg), ToastType::Error);
                    }),
                ) {
                    Ok(r) => {
                        *local_recorder.borrow_mut() = Some(r);
                        *recording_stream_id.borrow_mut() = Some(stream.id());
                        // No WS notification — the recording session continues
                        // from the other participants' perspective.
                    }
                    Err(e) => {
                        web_sys::console::error_1(&e);
                        *recording_stream_id.borrow_mut() = None;
                        set_is_recording_locally.set(false);
                        add_toast(
                            "Recording stopped: failed to record new stream".to_string(),
                            ToastType::Error,
                        );
                        if let Some(socket) = ws.get() {
                            let _ = socket.send_with_str(
                                &serde_json::to_string(&ClientMessage::ToggleLocalRecording(false))
                                    .unwrap(),
                            );
                        }
                    }
                }
            } else {
                // Stream was removed entirely (e.g. all media disabled)
                *recording_stream_id.borrow_mut() = None;
                set_is_recording_locally.set(false);
                add_toast(
                    "Recording stopped: media stream ended".to_string(),
                    ToastType::Info,
                );
                if let Some(socket) = ws.get() {
                    let _ = socket.send_with_str(
                        &serde_json::to_string(&ClientMessage::ToggleLocalRecording(false))
                            .unwrap(),
                    );
                }
            }
        });
    }

    let request_unmute = Callback::new(move |target_id: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::RequestUnmute(target_id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let update_power_status = Callback::new(move |status: shared::PowerStatus| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::UpdatePowerStatus(status);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let broadcast_to_lobby = Callback::new(move |text: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::BroadcastToLobby(text);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let promote_visitor = Callback::new(move |id: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::PromoteVisitor(id);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let analytics_for_camera = analytics.clone();
    let toggle_camera = Callback::new(move |_: ()| {
        if is_visitor.get_untracked() {
            return;
        }

        let is_currently_off = if let Some(stream) = local_stream.get_untracked() {
            stream.get_video_tracks().length() == 0
        } else {
            true
        };

        if is_currently_off {
            let is_mod_enabled = room_config.with_untracked(|c| c.video_moderation_enabled);
            let has_perm = has_camera_permission.get_untracked();
            if is_mod_enabled && !has_perm && !is_host.get_untracked() {
                add_toast(
                    "Camera is moderated. Request permission to enable.".to_string(),
                    ToastType::Error,
                );
                return;
            }
        }

        // If the camera was disabled by the host, re-enable the existing
        // tracks instead of restarting the media stream. This mirrors
        // toggle_mic which checks is_muted to determine the current state.
        if is_camera_off.get_untracked() {
            set_is_camera_off.set(false);
            analytics_for_camera.track_toggle_media("camera", true);
            if let Some(raw) = raw_local_stream.get_untracked() {
                let video_tracks = raw.get_video_tracks();
                for i in 0..video_tracks.length() {
                    if let Ok(track) = video_tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                        track.set_enabled(true);
                    }
                }
            }
            if let Some(stream) = local_stream.get_untracked() {
                let video_tracks = stream.get_video_tracks();
                for i in 0..video_tracks.length() {
                    if let Ok(track) = video_tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                        track.set_enabled(true);
                    }
                }
            }
            return;
        }

        // Check if we currently have video tracks active
        let has_video = if let Some(stream) = local_stream.get_untracked() {
            stream.get_video_tracks().length() > 0
        } else {
            false
        };
        let new_state = !has_video;
        analytics_for_camera.track_toggle_media("camera", new_state);

        if let Some(socket) = ws.get() {
            let msg = ClientMessage::SetCameraMuteStatus(!new_state);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }

        start_media_stream.run(new_state);
    });

    let set_input_devices = Callback::new(
        move |(vid, aid, res, ns): (Option<String>, Option<String>, String, bool)| {
            update_setting(|s| {
                s.camera_id = vid.clone();
                s.mic_id = aid.clone();
                s.resolution = Some(res.clone());
            });
            let old_ns = is_noise_suppression_enabled.get_untracked();
            set_selected_camera_id.set(vid);
            set_selected_mic_id.set(aid);
            set_video_resolution.set(res);
            set_is_noise_suppression_enabled.set(ns);

            let has_video = if let Some(stream) = local_stream.get_untracked() {
                stream.get_video_tracks().length() > 0
            } else {
                false
            };

            if local_stream.get_untracked().is_some() {
                let ns_will_trigger_restart = ns
                    && !old_ns
                    && audio_monitor.with_untracked(|m: &Option<AudioMonitor>| {
                        m.as_ref().is_some_and(|monitor| !monitor.has_compressor())
                    });
                if !ns_will_trigger_restart {
                    start_media_stream.run(has_video);
                }
            }
        },
    );

    let start_share_video = Callback::new(move |url: String| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::StartShareVideo(url);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let stop_share_video = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::StopShareVideo;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let analytics_for_mic = analytics.clone();
    let toggle_mic = Callback::new(move |_: ()| {
        if is_visitor.get_untracked() {
            return;
        }

        let is_currently_muted = is_muted.get_untracked();
        if is_currently_muted {
            let is_mod_enabled = room_config.with_untracked(|c| c.audio_moderation_enabled);
            let has_perm = has_unmute_permission.get_untracked();
            if is_mod_enabled && !has_perm && !is_host.get_untracked() {
                add_toast(
                    "Audio is moderated. Request permission to unmute.".to_string(),
                    ToastType::Error,
                );
                return;
            }
        }

        let new_state = !is_currently_muted;
        set_is_muted.set(new_state);
        analytics_for_mic.track_toggle_media("microphone", !new_state);

        if let Some(stream) = local_stream.get() {
            let audio_tracks = stream.get_audio_tracks();
            for i in 0..audio_tracks.length() {
                if let Ok(track) = audio_tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                    track.set_enabled(!new_state); // enabled = !muted
                }
            }

            set_audio_monitor.update(|monitor: &mut Option<AudioMonitor>| {
                if let Some(m) = monitor.as_mut() {
                    m.set_muted(new_state);
                }
            });
        }

        if let Some(socket) = ws.get() {
            let msg = ClientMessage::SetMuteStatus(new_state);
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let authenticate = Callback::new(move |(username, password): (String, Option<String>)| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::Authenticate { username, password };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let fetch_calendar = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            let msg = ClientMessage::FetchCalendar;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    let send_ping = Callback::new(move |_: ()| {
        if let Some(socket) = ws.get() {
            set_last_ping_time.set(js_sys::Date::now());
            let msg = ClientMessage::Ping;
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = socket.send_with_str(&json);
            }
        }
    });

    RoomState {
        connection_state: current_state,
        is_audio_moderated,
        is_video_moderated,
        has_unmute_permission,
        has_camera_permission,
        messages,
        participants,
        knocking_participants,
        is_connected,
        is_locked,
        is_e2ee_enabled,
        is_lobby_enabled,
        is_recording,
        is_subtitles_enabled,
        subtitles,
        show_settings,
        show_polls,
        show_shortcuts,
        polls,
        last_reaction,
        show_whiteboard,
        show_etherpad,
        whiteboard_history,
        whiteboard_reset,
        my_id,
        typing_users,
        is_host,
        host_id,
        current_room_id,
        breakout_rooms,
        is_authenticated,
        show_login_dialog,
        auth_error,
        calendar_events,
        show_calendar,
        local_stream,
        local_screen_stream,
        is_muted,
        shared_video_url,
        speaking_peers,
        audio_monitor,
        raw_local_stream,
        show_speaker_stats,
        show_virtual_background,
        show_feedback,
        rtt,
        audio_level,
        remote_streams,
        selected_camera_id,
        selected_mic_id,
        video_resolution,
        is_noise_suppression_enabled,
        background_mode,
        grid_layout,
        is_visitor,
        room_config,
        power_statuses,
        branding,
        is_recording_locally,
        is_talking_while_muted,
        show_rejoin,
        lobby_announcement,
        pending_unmute_requests,
        pending_camera_requests,
        dominant_speaker,
        _last_face_expression,
        is_face_landmarks_enabled,
        set_input_devices,
        set_background_mode,
        set_grid_layout,
        set_show_settings,
        set_show_polls,
        set_show_shortcuts,
        set_show_whiteboard,
        set_show_etherpad,
        set_show_speaker_stats,
        set_show_virtual_background,
        set_show_feedback,
        set_show_login_dialog,
        set_auth_error,
        authenticate,
        set_show_calendar,
        fetch_calendar,
        send_ping,
        send_message,
        toggle_lock,
        password_required,
        toggle_e2ee,
        toggle_participant_e2ee,
        toggle_etherpad,
        toggle_lobby,
        toggle_recording,
        toggle_subtitles,
        grant_access,
        deny_access,
        join_meeting,
        rejoin,
        save_profile,
        set_avatar_url,
        set_subject,
        send_reaction,
        toggle_raise_hand,
        toggle_screen_share,
        kick_participant,
        create_poll,
        vote_poll,
        close_poll,
        send_draw,
        set_is_typing,
        create_breakout_room,
        remove_breakout_room,
        rename_breakout_room,
        join_breakout_room,
        close_all_breakout_rooms,
        move_participant_to_room,
        auto_assign_to_breakout_rooms,
        toggle_camera,
        toggle_mic,
        end_meeting,
        mute_participant,
        mute_camera_participant,
        mute_all,
        mute_camera_all,
        stop_screen_share_all,
        set_branding,
        transfer_host,
        start_share_video,
        stop_share_video,
        set_presence,
        toggle_local_recording,
        request_unmute,
        update_power_status,
        broadcast_to_lobby,
        promote_visitor,
        analytics,
        _face_landmarks: face_landmarks,
        remote_control,
        is_audio_only,
        is_flipped,
        pinned_participant,
        participant_volumes,
        toggle_audio_only,
        toggle_flip,
        pin_participant,
        set_participant_volume,
        mute_everyone_else,
        request_unmute_permission,
        request_camera_permission,
        grant_unmute_permission,
        grant_camera_permission,
        toggle_audio_moderation,
        toggle_video_moderation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_connection_state_equality() {
        assert_eq!(RoomConnectionState::Prejoin, RoomConnectionState::Prejoin);
        assert_ne!(RoomConnectionState::Prejoin, RoomConnectionState::Joined);
    }
}
