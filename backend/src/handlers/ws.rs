use super::breakout;
use super::calendar;
use super::chat;
use super::dropbox;
use super::moderation;
use super::polls;
use super::remote_control;
use super::salesforce;
use super::whiteboard;
use crate::AppState;
use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use shared::{ClientMessage, Participant, RoomConfig, ServerMessage};
use std::sync::Arc;
use tracing;

/// Upper bound on a single inbound WebSocket message. axum already defaults to
/// 64 MiB, but that is far larger than any legitimate client message and lets a
/// single frame carry a multi-megabyte payload, so we set an explicit, tighter
/// limit.
const MAX_WS_MESSAGE_SIZE: usize = 4 * 1024 * 1024;

pub async fn chat_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.max_message_size(MAX_WS_MESSAGE_SIZE)
        .on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Channel for internal messages to self
    let (internal_tx, mut internal_rx) = tokio::sync::mpsc::channel::<ServerMessage>(10);

    let current_config: RoomConfig = {
        let config = state.room_config.lock().unwrap();
        config.clone()
    };
    if let Ok(json) = serde_json::to_string(&ServerMessage::RoomUpdated(current_config.clone())) {
        let _ = sender.send(Message::Text(json.into())).await;
    }

    // Explicitly send RoomUpdated to self to trigger frontend state logic (like is_host)
    let _ = internal_tx
        .send(ServerMessage::RoomUpdated(current_config.clone()))
        .await;

    // Channel for control messages from async tasks to the loop
    let (control_tx, mut control_rx) = tokio::sync::mpsc::channel::<bool>(1); // true = granted, false = denied

    // Send loop
    let send_task = tokio::spawn(async move {
        while let Some(msg) = internal_rx.recv().await {
            if let Ok(json_msg) = serde_json::to_string(&msg)
                && sender.send(Message::Text(json_msg.into())).await.is_err()
            {
                break;
            }
        }
    });

    // Receive loop
    let tx = state.tx.clone();
    let participants_mutex = state.participants.clone();
    let knocking_mutex = state.knocking_participants.clone();
    let room_config_mutex = state.room_config.clone();
    let polls_mutex = state.polls.clone();
    let whiteboard_mutex = state.whiteboard.clone();
    let chat_history_mutex = state.chat_history.clone();
    let breakout_rooms_mutex = state.breakout_rooms.clone();
    let participant_locations_mutex = state.participant_locations.clone();
    let shared_video_mutex = state.shared_video_url.clone();
    let speaking_start_times_mutex = state.speaking_start_times.clone();

    // We don't have an ID yet
    let mut my_id: Option<String> = None;
    let mut knocking_id: Option<String> = None;
    // Track my current room locally for quick access
    let mut my_room_id: Option<String> = None;
    let mut broadcast_task: Option<tokio::task::JoinHandle<()>> = None;
    // Rate limiting for analytics events: max 10 events per second per connection
    let mut analytics_count: u32 = 0;
    let mut analytics_window_start = std::time::Instant::now();
    // Separate rate limiter for high-frequency `RemoteControlAction`
    // messages (max 30/sec) so an active session does not starve other
    // analytics-class messages (UpdatePowerStatus, ToggleLocalRecording,
    // AnalyticsEvent) sharing the analytics counter.
    let mut rc_count: u32 = 0;
    let mut rc_window_start = std::time::Instant::now();
    // Dedicated rate limiter for `FaceExpression` messages (max 10/sec)
    // so they don't share a budget with UpdatePowerStatus / ToggleLocalRecording
    // / AnalyticsEvent. If the mock detection frequency is ever increased,
    // this prevents face expressions from starving toast-bearing messages
    // like ToggleLocalRecording.
    let mut face_count: u32 = 0;
    let mut face_window_start = std::time::Instant::now();
    // Track the last recording state sent by this client to prevent
    // redundant broadcasts (and toast spam) from repeated identical
    // ToggleLocalRecording messages.
    let mut last_recording_state: Option<bool> = None;

    // Send initial breakout rooms list
    let rooms: Vec<shared::BreakoutRoom> = {
        let rooms = breakout_rooms_mutex.lock().unwrap();
        rooms.values().cloned().collect()
    };
    if !rooms.is_empty() {
        // Send via internal_tx to avoid "borrow of moved value: sender"
        let _ = internal_tx
            .send(ServerMessage::BreakoutRoomsList(rooms))
            .await;
    }

    loop {
        tokio::select! {
            res = receiver.next() => {
                match res {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                            match client_msg {
                                ClientMessage::KickParticipant(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let host_id = {
                                            room_config_mutex.lock().unwrap().host_id.clone()
                                        };
                                        if Some(uid.clone()) == host_id {
                                            if target_id == *uid {
                                                // Prevent self-kick
                                                continue;
                                            }
                                            // Valid kick
                                            // 1. Update speaking time before removal
                                            {
                                                let mut starts = speaking_start_times_mutex.lock().unwrap();
                                                if let Some(start) = starts.remove(&target_id) {
                                                    let now = chrono::Utc::now().timestamp_millis() as u64;
                                                    if now > start {
                                                        let delta = now - start;
                                                        let mut participants = participants_mutex.lock().unwrap();
                                                        if let Some(p) = participants.get_mut(&target_id) {
                                                            p.speaking_time += delta;
                                                            // Broadcast final update before kick
                                                            let _ = tx.send(ServerMessage::ParticipantUpdated(p.clone()));
                                                        }
                                                    }
                                                }
                                            }
                                            // 2. Remove from participants
                                            {
                                                let mut participants = participants_mutex.lock().unwrap();
                                                participants.remove(&target_id);
                                            }
                                            // Fetch target's location before removal to broadcast Left accurately
                                            let target_loc = {
                                                let locations = participant_locations_mutex.lock().unwrap();
                                                locations.get(&target_id).cloned().flatten()
                                            };
                                            // 3. Remove from participant_locations
                                            {
                                                let mut locations = participant_locations_mutex.lock().unwrap();
                                                locations.remove(&target_id);
                                            }
                                            // 3b. Clean up any remote-control sessions and pending
                                            // requests involving the kicked participant so stale
                                            // entries do not authorize future actions across rooms
                                            // (e.g. on UUID collision) and so leaked sessions don't
                                            // accumulate.
                                            {
                                                let mut sessions = state.remote_control_sessions.lock().unwrap();
                                                sessions.remove(&target_id);
                                                sessions.retain(|_, controlled| controlled != &target_id);
                                            }
                                            {
                                                let mut pending = state.pending_remote_control_requests.lock().unwrap();
                                                pending.retain(|(req, tgt)| req != &target_id && tgt != &target_id);
                                            }
                                            // 4. Broadcast Kicked
                                            let _ = tx.send(ServerMessage::Kicked { target_id: target_id.clone(), room_id: target_loc.clone() });

                                            // 5. Broadcast ParticipantLeft (so lists update)
                                            let _ = tx.send(ServerMessage::ParticipantLeft { id: target_id, room_id: target_loc });
                                        }
                                    }
                                },
                                ClientMessage::E2EEKeyExchange(key_hash) => {
                                    if let Some(uid) = &my_id {
                                        let _ = tx.send(ServerMessage::E2EEKeyExchange {
                                            from_id: uid.clone(),
                                            key_hash,
                                        });
                                    }
                                },
                                ClientMessage::LinkSalesforce(config) => {
                                    if let Some(uid) = &my_id {
                                        let msgs = salesforce::handle_link_salesforce(uid, config, &state);
                                        for m in msgs {
                                            let _ = tx.send(m);
                                        }
                                    }
                                },
                                ClientMessage::SaveToDropbox(filename) => {
                                    if let Some(uid) = &my_id {
                                        let msgs = dropbox::handle_save_to_dropbox(uid, filename, &state);
                                        for m in msgs {
                                            let _ = internal_tx.send(m).await;
                                        }
                                    }
                                },
                                ClientMessage::ToggleAudioModeration => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let new_config = {
                                                let mut config = room_config_mutex.lock().unwrap();
                                                config.audio_moderation_enabled = !config.audio_moderation_enabled;
                                                if !config.audio_moderation_enabled {
                                                    state.unmute_permissions.lock().unwrap().clear();
                                                }
                                                config.clone()
                                            };
                                            let _ = tx.send(ServerMessage::RoomUpdated(new_config));
                                        }
                                    }
                                },
                                ClientMessage::ToggleVideoModeration => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let new_config = {
                                                let mut config = room_config_mutex.lock().unwrap();
                                                config.video_moderation_enabled = !config.video_moderation_enabled;
                                                if !config.video_moderation_enabled {
                                                    state.camera_permissions.lock().unwrap().clear();
                                                }
                                                config.clone()
                                            };
                                            let _ = tx.send(ServerMessage::RoomUpdated(new_config));
                                        }
                                    }
                                },
                                ClientMessage::RemoveBreakoutRoom(room_id) => {
                                    if let Some(uid) = &my_id {
                                        match breakout::remove_breakout_room(uid, room_id, &state) {
                                            Ok(msgs) => { for m in msgs { let _ = tx.send(m); } },
                                            Err(e) => { let _ = internal_tx.send(ServerMessage::Error(e)).await; }
                                        }
                                    }
                                },
                                ClientMessage::RenameBreakoutRoom { room_id, new_name } => {
                                    if let Some(uid) = &my_id {
                                        match breakout::rename_breakout_room(uid, room_id, new_name, &state) {
                                            Ok(msg) => { let _ = tx.send(msg); },
                                            Err(e) => { let _ = internal_tx.send(ServerMessage::Error(e)).await; }
                                        }
                                    }
                                },
                                ClientMessage::StopScreenShareAll => {
                                    if let Some(uid) = &my_id {
                                        let msgs = moderation::stop_screen_share_all(uid, &state);
                                        for msg in msgs {
                                            let _ = tx.send(msg);
                                        }
                                    }
                                },
                                ClientMessage::SetBranding(branding) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            // Colours are written straight into CSS custom
                                            // properties on every participant's client and the
                                            // logo into an <img src>, so validate both before
                                            // accepting them: only hex colours and https URLs.
                                            if let Some(ref color) = branding.primary_color
                                                && !shared::is_valid_css_hex_color(color)
                                            {
                                                let _ = internal_tx.send(ServerMessage::Error("Invalid primary color: expected a hex value like #RRGGBB".to_string())).await;
                                                continue;
                                            }
                                            if let Some(ref color) = branding.background_color
                                                && !shared::is_valid_css_hex_color(color)
                                            {
                                                let _ = internal_tx.send(ServerMessage::Error("Invalid background color: expected a hex value like #RRGGBB".to_string())).await;
                                                continue;
                                            }
                                            if let Some(ref logo) = branding.logo_url
                                                && !shared::is_valid_logo_url(logo)
                                            {
                                                let _ = internal_tx.send(ServerMessage::Error("Invalid logo URL: must start with https://".to_string())).await;
                                                continue;
                                            }
                                            let new_config = {
                                                let mut config = room_config_mutex.lock().unwrap();
                                                config.branding = branding;
                                                config.clone()
                                            };
                                            let _ = tx.send(ServerMessage::RoomUpdated(new_config));
                                        }
                                    }
                                },
                                ClientMessage::SetSubject(subject) => {
                                    // Validate subject length to prevent abuse. Use char count
                                    // (not byte length) so multi-byte UTF-8 content (CJK, emoji)
                                    // is treated consistently with what users see.
                                    if let Some(ref s) = subject
                                        && s.chars().count() > 256 {
                                            let _ = internal_tx.send(ServerMessage::Error("Invalid subject: too long".to_string())).await;
                                            continue;
                                        }
                                    // Normalize `Some("")` to `None` for defensive consistency
                                    // with the frontend (state.rs `set_subject`), so a client
                                    // bypassing the frontend cannot store an empty-string
                                    // subject in the room config.
                                    let subject = subject.filter(|s| !s.is_empty());
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            // Clone the updated config inside a nested block
                                            // so the mutex guard is dropped before broadcasting,
                                            // matching the pattern used elsewhere (e.g. ToggleLobby).
                                            let new_config = {
                                                let mut config = room_config_mutex.lock().unwrap();
                                                config.subject = subject;
                                                config.clone()
                                            };
                                            let _ = tx.send(ServerMessage::RoomUpdated(new_config));
                                        }
                                    }
                                },
                                ClientMessage::UpdateAvatar(url) => {
                                    // Server-side avatar URL validation: must be https
                                    // and within a reasonable length, to prevent abuse where
                                    // a participant could force other clients to issue
                                    // requests to arbitrary URLs (e.g. IP-leaking trackers).
                                    // Plain http:// is rejected to avoid mixed-content
                                    // loading and MITM avatar swaps when the meeting is
                                    // served over HTTPS.
                                    if let Some(ref url_str) = url {
                                        if !url_str.starts_with("https://") {
                                            let _ = internal_tx.send(ServerMessage::Error("Invalid avatar URL: must start with https://".to_string())).await;
                                            continue;
                                        }
                                        if url_str.len() > 2048 {
                                            let _ = internal_tx.send(ServerMessage::Error("Invalid avatar URL: too long".to_string())).await;
                                            continue;
                                        }
                                    }
                                    if let Some(uid) = &my_id {
                                        let updated_p = {
                                            let mut participants = participants_mutex.lock().unwrap();
                                            if let Some(p) = participants.get_mut(uid) {
                                                p.avatar_url = url;
                                                Some(p.clone())
                                            } else {
                                                None
                                            }
                                        };
                                        if let Some(p) = updated_p {
                                            let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                        }
                                    }
                                },
                                ClientMessage::FaceExpression(expression) => {
                                    if let Some(uid) = &my_id {
                                        // Rate limit with a dedicated counter (max 10/sec
                                        // per connection) so face expressions do not share
                                        // a budget with UpdatePowerStatus / ToggleLocalRecording
                                        // / AnalyticsEvent.
                                        let now = std::time::Instant::now();
                                        if now.duration_since(face_window_start) >= std::time::Duration::from_secs(1) {
                                            face_count = 0;
                                            face_window_start = now;
                                        }
                                        face_count += 1;
                                        if face_count > 10 {
                                            continue;
                                        }
                                        let _ = tx.send(ServerMessage::FaceExpression {
                                            sender_id: uid.clone(),
                                            expression,
                                        });
                                    }
                                },
                                ClientMessage::MuteCameraParticipant(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let same_room = {
                                                let locs = participant_locations_mutex.lock().unwrap();
                                                let host_loc = locs.get(uid).cloned().flatten();
                                                let target_loc = locs.get(&target_id).cloned().flatten();
                                                host_loc == target_loc
                                            };
                                            if !same_room {
                                                continue;
                                            }
                                            let updated_p = {
                                                let mut participants = participants_mutex.lock().unwrap();
                                                if let Some(p) = participants.get_mut(&target_id) {
                                                    p.is_camera_muted = true;
                                                    Some(p.clone())
                                                } else {
                                                    None
                                                }
                                            };
                                            if let Some(p) = updated_p {
                                                let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                                let _ = tx.send(ServerMessage::CameraMutedByHost(target_id));
                                            }
                                        }
                                    }
                                },
                                ClientMessage::BroadcastToLobby(text) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let _ = tx.send(ServerMessage::LobbyAnnouncement(text));
                                        }
                                    }
                                },
                                ClientMessage::MuteCameraAll => {
                                    if let Some(uid) = &my_id {
                                        let msgs = moderation::mute_camera_all(uid, &state);
                                        for msg in msgs {
                                            let _ = tx.send(msg);
                                        }
                                    }
                                },
                                ClientMessage::PromoteVisitor(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let updated_p = {
                                                let mut participants = participants_mutex.lock().unwrap();
                                                if let Some(p) = participants.get_mut(&target_id) {
                                                    p.is_visitor = false;
                                                    Some(p.clone())
                                                } else {
                                                    None
                                                }
                                            };
                                            if let Some(p) = updated_p {
                                                let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                                let _ = tx.send(ServerMessage::VisitorPromoted(target_id));
                                            }
                                        }
                                    }
                                },
                                ClientMessage::FollowMe(layout) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let _ = tx.send(ServerMessage::FollowMe(layout));
                                        }
                                    }
                                },
                                ClientMessage::UpdatePowerStatus(mut status) => {
                                    if let Some(uid) = &my_id {
                                        // Rate limit: reuse the analytics rate limiter to
                                        // prevent abuse from malicious clients.
                                        let now = std::time::Instant::now();
                                        if now.duration_since(analytics_window_start) >= std::time::Duration::from_secs(1) {
                                            analytics_count = 0;
                                            analytics_window_start = now;
                                        }
                                        analytics_count += 1;
                                        if analytics_count > 10 {
                                            continue;
                                        }
                                        // Clamp battery_level to valid range; NaN becomes 0.0
                                        if status.battery_level.is_nan() {
                                            status.battery_level = 0.0;
                                        } else {
                                            status.battery_level = status.battery_level.clamp(0.0, 1.0);
                                        }
                                        let _ = tx.send(ServerMessage::PowerStatusUpdated {
                                            user_id: uid.clone(),
                                            status,
                                        });
                                    }
                                },
                                ClientMessage::RequestUnmute(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            // Only allow unmute requests within the same breakout
                                            // room, consistent with MuteParticipant scoping.
                                            let same_room = {
                                                let locs = participant_locations_mutex.lock().unwrap();
                                                let host_loc = locs.get(uid).cloned().flatten();
                                                let target_loc = locs.get(&target_id).cloned().flatten();
                                                host_loc == target_loc
                                            };
                                            if !same_room {
                                                continue;
                                            }
                                            let _ = tx.send(ServerMessage::UnmuteRequested {
                                                requester_id: uid.clone(),
                                                target_id,
                                            });
                                        }
                                    }
                                },
                                ClientMessage::ToggleLocalRecording(is_recording) => {
                                    if let Some(uid) = &my_id {
                                        // Deduplicate: skip if the client is re-sending the
                                        // same recording state it already sent. This prevents
                                        // toast spam from malicious or buggy clients.
                                        if last_recording_state == Some(is_recording) {
                                            continue;
                                        }
                                        // Rate limit: reuse the analytics rate limiter to
                                        // prevent toast spam from malicious clients.
                                        let now = std::time::Instant::now();
                                        if now.duration_since(analytics_window_start) >= std::time::Duration::from_secs(1) {
                                            analytics_count = 0;
                                            analytics_window_start = now;
                                        }
                                        analytics_count += 1;
                                        if analytics_count > 10 {
                                            continue;
                                        }
                                        last_recording_state = Some(is_recording);
                                        let _ = tx.send(ServerMessage::RecordingStatusChanged {
                                            user_id: uid.clone(),
                                            is_recording,
                                        });
                                    }
                                },
                                // NOTE: UpdateE2EE is handled server-side but no
                                // frontend UI currently sends this message. It is
                                // kept for protocol completeness; wire up when the
                                // per-participant E2EE settings panel is migrated.
                                ClientMessage::UpdateE2EE(enabled) => {
                                    if let Some(uid) = &my_id {
                                        let updated_participant = {
                                            let mut participants = participants_mutex.lock().unwrap();
                                            if let Some(p) = participants.get_mut(uid) {
                                                p.e2ee_enabled = enabled;
                                                Some(p.clone())
                                            } else {
                                                None
                                            }
                                        };
                                        if let Some(p) = updated_participant {
                                            let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                        }
                                    }
                                },
                                ClientMessage::ToggleE2EE => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let new_config = {
                                                let mut config = room_config_mutex.lock().unwrap();
                                                config.e2ee_enabled = !config.e2ee_enabled;
                                                config.clone()
                                            };
                                            let _ = tx.send(ServerMessage::RoomUpdated(new_config));
                                        }
                                    }
                                },
                                ClientMessage::SetEtherpadUrl(url) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            // Server-side URL validation
                                            if let Some(ref url_str) = url {
                                                if !url_str.starts_with("https://") && !url_str.starts_with("http://") {
                                                    let _ = internal_tx.send(ServerMessage::Error("Invalid Etherpad URL: must start with http:// or https://".to_string())).await;
                                                    continue;
                                                }
                                                if url_str.len() > 2048 {
                                                    let _ = internal_tx.send(ServerMessage::Error("Invalid Etherpad URL: too long".to_string())).await;
                                                    continue;
                                                }
                                            }
                                            let config = {
                                                let mut config = room_config_mutex.lock().unwrap();
                                                config.etherpad_url = url.clone();
                                                config.clone()
                                            };
                                            let _ = tx.send(ServerMessage::RoomUpdated(config));
                                        }
                                    }
                                },
                                ClientMessage::GiphyShare(_) => {
                                    // GIFs are sent via ClientMessage::Chat with a "GIF:" prefix;
                                    // this dedicated variant is unused by the frontend.
                                },
                                ClientMessage::ToggleSubtitles => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let config = {
                                                let mut config = room_config_mutex.lock().unwrap();
                                                config.is_subtitles_enabled = !config.is_subtitles_enabled;
                                                config.clone()
                                            };
                                            let _ = tx.send(ServerMessage::RoomUpdated(config));
                                        }
                                    }
                                },
                                ClientMessage::SetPresence(status) => {
                                    if let Some(uid) = &my_id {
                                        let updated_participant = {
                                            let mut participants = participants_mutex.lock().unwrap();
                                            if let Some(p) = participants.get_mut(uid) {
                                                p.presence = status;
                                                Some(p.clone())
                                            } else {
                                                None
                                            }
                                        };

                                        if let Some(p) = updated_participant {
                                            let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                        }
                                    }
                                },
                                ClientMessage::EndMeeting => {
                                    if let Some(uid) = &my_id {
                                        let host_id = {
                                            room_config_mutex.lock().unwrap().host_id.clone()
                                        };
                                        if Some(uid.clone()) == host_id {
                                            // Valid end meeting
                                            // Broadcast RoomEnded
                                            let _ = tx.send(ServerMessage::RoomEnded);

                                            {
                                                let mut p = participants_mutex.lock().unwrap();
                                                p.clear();
                                            }
                                            {
                                                let mut c = room_config_mutex.lock().unwrap();
                                                *c = shared::RoomConfig::default();
                                            }
                                            {
                                                let mut s = speaking_start_times_mutex.lock().unwrap();
                                                s.clear();
                                            }
                                            {
                                                let mut k = state.knocking_participants.lock().unwrap();
                                                k.clear();
                                            }
                                            {
                                                let mut p = state.polls.lock().unwrap();
                                                p.clear();
                                            }
                                            {
                                                let mut w = state.whiteboard.lock().unwrap();
                                                w.clear();
                                            }
                                            {
                                                let mut ch = state.chat_history.lock().unwrap();
                                                ch.clear();
                                            }
                                            {
                                                let mut br = state.breakout_rooms.lock().unwrap();
                                                br.clear();
                                            }
                                            {
                                                let mut loc = state.participant_locations.lock().unwrap();
                                                loc.clear();
                                            }
                                            {
                                                let mut vid = state.shared_video_url.lock().unwrap();
                                                *vid = None;
                                            }
                                            {
                                                let mut rc = state.remote_control_sessions.lock().unwrap();
                                                rc.clear();
                                            }
                                            {
                                                let mut pending = state.pending_remote_control_requests.lock().unwrap();
                                                pending.clear();
                                            }
                                            {
                                                let mut fb = state.feedback.lock().unwrap();
                                                fb.clear();
                                            }
                                        }
                                    }
                                },
                                ClientMessage::ToggleLobby => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let new_config = {
                                                let mut config = room_config_mutex.lock().unwrap();
                                                config.is_lobby_enabled = !config.is_lobby_enabled;
                                                config.clone()
                                            };
                                            let _ = tx.send(ServerMessage::RoomUpdated(new_config));
                                        }
                                    }
                                },
                                ClientMessage::GrantAccess(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let sender_opt = {
                                                let mut knocking = knocking_mutex.lock().unwrap();
                                                knocking.get_mut(&target_id).and_then(|(_, s)| s.take())
                                            };
                                            if let Some(s) = sender_opt {
                                                let _ = s.send(true);
                                            }
                                        }
                                    }
                                },
                                ClientMessage::DenyAccess(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let sender_opt = {
                                                let mut knocking = knocking_mutex.lock().unwrap();
                                                knocking.get_mut(&target_id).and_then(|(_, s)| s.take())
                                            };
                                            if let Some(s) = sender_opt {
                                                let _ = s.send(false);
                                            }
                                        }
                                    }
                                },
                                ClientMessage::Join { name, is_visitor, avatar_url, password } => {
                                    if my_id.is_some() || knocking_id.is_some() { continue; } // Already joined or knocking

                                    // Cap the display name so a client cannot bloat
                                    // every participant's roster with an unbounded string.
                                    if name.chars().count() > shared::MAX_PARTICIPANT_NAME_LEN {
                                        let _ = internal_tx.send(ServerMessage::Error("Name too long".to_string())).await;
                                        continue;
                                    }

                                    // Sanitize avatar URL on join: since `avatar_url` is an
                                    // optional cosmetic field, an invalid value should not
                                    // prevent the user from joining the room. Drop the URL
                                    // (treat as `None`) when it fails the same validation
                                    // rules as `UpdateAvatar` (https scheme only, ≤2048
                                    // chars). Clients can still recover via `UpdateAvatar`
                                    // afterwards, which surfaces an explicit error.
                                    let avatar_url = avatar_url.filter(|url_str| {
                                        url_str.starts_with("https://") && url_str.len() <= 2048
                                    });

                                    // Check if room is locked or lobby is enabled
                                    let (is_locked, room_password, is_lobby, max_participants, host_exists) = {
                                        let config = room_config_mutex.lock().unwrap();
                                        (
                                            config.is_locked,
                                            config.access_password.clone(),
                                            config.is_lobby_enabled,
                                            config.max_participants,
                                            config.host_id.is_some(),
                                        )
                                    };

                                    if is_locked {
                                        match &room_password {
                                            // Password-protected lock: accept only an exact match.
                                            Some(expected) => {
                                                match password.as_deref() {
                                                    Some(provided) if provided == expected => {
                                                        // Correct password: fall through to join.
                                                    }
                                                    Some(_) => {
                                                        let _ = internal_tx
                                                            .send(ServerMessage::Error(
                                                                "Invalid room password".to_string(),
                                                            ))
                                                            .await;
                                                        continue;
                                                    }
                                                    None => {
                                                        let _ = internal_tx
                                                            .send(ServerMessage::Error(
                                                                "Password required".to_string(),
                                                            ))
                                                            .await;
                                                        continue;
                                                    }
                                                }
                                            }
                                            // Hard lock without a password: no way in.
                                            None => {
                                                let _ = internal_tx
                                                    .send(ServerMessage::Error(
                                                        "Room is locked".to_string(),
                                                    ))
                                                    .await;
                                                continue;
                                            }
                                        }
                                    }

                                    let id = uuid::Uuid::new_v4().to_string();
                                    let me = Participant {
                                        id: id.clone(),
                                        name,
                                        is_hand_raised: false,
                                        is_sharing_screen: false,
                                        is_muted: false,
                                        is_camera_muted: false,
                                        speaking_time: 0,
                                        presence: shared::PresenceStatus::Connected,
                                        is_visitor,
                                        e2ee_enabled: false,
                                        hand_raised_at: None,
                                        avatar_url,
                                    };

                                    if is_lobby && host_exists {
                                        let (s, r) = tokio::sync::oneshot::channel();
                                        {
                                            let mut knocking = knocking_mutex.lock().unwrap();
                            knocking.insert(id.clone(), (me.clone(), Some(s)));
                                        }
                                        knocking_id = Some(id.clone());
                                        let _ = internal_tx.send(ServerMessage::Knocking).await;
                                        let _ = tx.send(ServerMessage::KnockingParticipant(me.clone()));

                                        let control_tx_clone = control_tx.clone();
                                        let knocking_mutex_clone = knocking_mutex.clone();
                                        let tx_clone = tx.clone();
                                        let id_clone = id.clone();
                                        let mut rx = tx.subscribe();
                                        let forward_tx = internal_tx.clone();

                                        tokio::spawn(async move {
                                            let mut r_fused = r;
                                            let timeout = tokio::time::sleep(std::time::Duration::from_secs(120));
                                            tokio::pin!(timeout);
                                            loop {
                                                tokio::select! {
                                                    res = &mut r_fused => {
                                                        match res {
                                                            Ok(true) => {
                                                                let _ = control_tx_clone.send(true).await;
                                                            },
                                                            _ => {
                                                                let removed = {
                                                                    let mut knocking = knocking_mutex_clone.lock().unwrap();
                                                                    knocking.remove(&id_clone).is_some()
                                                                };
                                                                if removed {
                                                                    let _ = tx_clone.send(ServerMessage::KnockingParticipantLeft(id_clone));
                                                                }
                                                                let _ = control_tx_clone.send(false).await;
                                                            }
                                                        }
                                                        break;
                                                    },
                                                    msg_res = rx.recv() => {
                                                        match msg_res {
                                                            Ok(ServerMessage::LobbyAnnouncement(text)) => {
                                                                let _ = forward_tx.send(ServerMessage::LobbyAnnouncement(text)).await;
                                                            },
                                                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                                            _ => {} // Ignore other messages and Lagged errors
                                                        }
                                                    },
                                                    _ = &mut timeout => {
                                                        let removed = {
                                                            let mut knocking = knocking_mutex_clone.lock().unwrap();
                                                            knocking.remove(&id_clone).is_some()
                                                        };
                                                        if removed {
                                                            let _ = tx_clone.send(ServerMessage::KnockingParticipantLeft(id_clone));
                                                        }
                                                        let _ = control_tx_clone.send(false).await;
                                                        break;
                                                    }
                                                }
                                            }
                                        });
                                        continue;
                                    }

                                    // Logic for direct join (no lobby)
                                    let (joined, new_host_assigned) = {
                                        let mut participants = participants_mutex.lock().unwrap();
                                        if participants.len() >= max_participants as usize {
                                            (false, false)
                                        } else {
                                            let mut config = room_config_mutex.lock().unwrap();
                                            // Robust Host Check: Clear host_id if participant no longer exists
                                            if let Some(hid) = &config.host_id
                                                && !participants.contains_key(hid) {
                                                    config.host_id = None;
                                                }
                                            let assigned = if config.host_id.is_none() && !me.is_visitor {
                                                config.host_id = Some(id.clone());
                                                true
                                            } else {
                                                false
                                            };

                                            participants.insert(id.clone(), me.clone());
                                            (true, assigned)
                                        }
                                    };

                                    if !joined {
                                        let _ = internal_tx.send(ServerMessage::Error("Room is full".to_string())).await;
                                        continue;
                                    }
                                    my_id = Some(id.clone());

                                    // Send Welcome with own ID
                                    let _ = internal_tx.send(ServerMessage::Welcome { id: id.clone() }).await;

                                    // Send Chat History
                                    let history: Vec<shared::ChatMessage> = {
                                        let history = chat_history_mutex.lock().unwrap();
                                        history.clone()
                                    };
                                    if !history.is_empty() {
                                        let _ = internal_tx.send(ServerMessage::ChatHistory(history)).await;
                                    }

                                    let current_config = {
                                        room_config_mutex.lock().unwrap().clone()
                                    };
                                    if new_host_assigned {
                                        let _ = tx.send(ServerMessage::RoomUpdated(current_config.clone()));
                                    }
                                    let _ = internal_tx.send(ServerMessage::RoomUpdated(current_config)).await;

                                    // Register initial location (Main Room)
                                    {
                                        let mut locations = participant_locations_mutex.lock().unwrap();
                                        locations.insert(id.clone(), None);
                                    }

                                    let mut rx = tx.subscribe();
                                    let forward_tx = internal_tx.clone();
                                    let my_id_clone = id.clone();
                                    let room_config_for_task = room_config_mutex.clone();
                                    let locations_clone = participant_locations_mutex.clone();
                                    let whiteboard_for_task = whiteboard_mutex.clone();

                                    broadcast_task = Some(tokio::spawn(async move {
                                        loop {
                                            match rx.recv().await {
                                                Ok(msg) => {
                                                    // Filter based on room and recipient
                                                    let should_send = match &msg {
                                                        ServerMessage::Chat { message, room_id } => {
                                                            let my_loc = {
                                                                let locs = locations_clone.lock().unwrap();
                                                                locs.get(&my_id_clone).cloned().flatten()
                                                            };
                                                            if *room_id != my_loc {
                                                                false
                                                            } else if let Some(target) = &message.recipient_id {
                                                                *target == my_id_clone || message.user_id == my_id_clone // Must echo private message back to self
                                                            } else {
                                                                true
                                                            }
                                                        },
                                                        ServerMessage::PeerTyping { room_id, .. } => {
                                                            let my_loc = {
                                                                let locs = locations_clone.lock().unwrap();
                                                                locs.get(&my_id_clone).cloned().flatten()
                                                            };
                                                            *room_id == my_loc
                                                        },
                                                        ServerMessage::Offer { source_id, target_id, .. }
                                                        | ServerMessage::Answer { source_id, target_id, .. }
                                                        | ServerMessage::IceCandidate { source_id, target_id, .. } => {
                                                            if *target_id == my_id_clone {
                                                                let locs = locations_clone.lock().unwrap();
                                                                let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                                let source_loc = locs.get(source_id).cloned().flatten();
                                                                my_loc == source_loc
                                                            } else {
                                                                false
                                                            }
                                                        },
                                                        ServerMessage::ParticipantJoined(p) => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            let source_loc = locs.get(&p.id).cloned().flatten();
                                                            my_loc == source_loc
                                                        },
                                                        ServerMessage::ParticipantUpdated(p) => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            let source_loc = locs.get(&p.id).cloned().flatten();
                                                            // Deliver to same room OR if it's an update about myself
                                                            my_loc == source_loc || p.id == my_id_clone
                                                        },
                                                        ServerMessage::ParticipantLeft { id, room_id } => {
                                                            // Don't deliver ParticipantLeft to the person who is leaving (e.g. during room switch)
                                                            if *id == my_id_clone {
                                                                false
                                                            } else {
                                                                let locs = locations_clone.lock().unwrap();
                                                                let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                                // Use the room_id embedded in the message instead of looking it up
                                                                my_loc == *room_id
                                                            }
                                                        },
                                                        ServerMessage::Kicked { target_id: id, room_id } => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            // Deliver to same room OR if it's a command directed at myself
                                                            my_loc == *room_id || *id == my_id_clone
                                                        },
                                                        ServerMessage::ForcedMoveToRoom { target_id, .. } => {
                                                            *target_id == my_id_clone
                                                        },
                                                        // Note: EtherpadUrlUpdated and GiphyShared are
                                                        // currently never broadcast by the server (etherpad
                                                        // changes go via RoomUpdated, GIFs via Chat). These
                                                        // arms are kept for protocol completeness.
                                                        ServerMessage::EtherpadUrlUpdated { room_id, .. } => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            my_loc == *room_id
                                                        },
                                                        ServerMessage::GiphyShared { room_id, .. } => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            my_loc == *room_id
                                                        },
                                                        ServerMessage::MutedByHost(id) | ServerMessage::CameraMutedByHost(id) => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            let source_loc = locs.get(id).cloned().flatten();
                                                            // Deliver to same room OR if it's a command directed at myself
                                                            my_loc == source_loc || *id == my_id_clone
                                                        },
                                                        ServerMessage::PowerStatusUpdated { user_id, .. } => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            let source_loc = locs.get(user_id).cloned().flatten();
                                                            my_loc == source_loc
                                                        },
                                                        ServerMessage::RecordingStatusChanged { user_id, .. } => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            let source_loc = locs.get(user_id).cloned().flatten();
                                                            my_loc == source_loc
                                                        },
                                                        ServerMessage::UnmuteRequested { target_id, .. } => {
                                                            *target_id == my_id_clone
                                                        },
                                                        ServerMessage::UnmutePermissionRequested { .. } => {
                                                            let config = room_config_for_task.lock().unwrap();
                                                            config.host_id == Some(my_id_clone.clone())
                                                        },
                                                        ServerMessage::CameraPermissionRequested { .. } => {
                                                            let config = room_config_for_task.lock().unwrap();
                                                            config.host_id == Some(my_id_clone.clone())
                                                        },
                                                        ServerMessage::PermissionGranted { target_id, .. } => {
                                                            *target_id == my_id_clone
                                                        },
                                                        ServerMessage::RemoteControlRequest { target_id, .. } => {
                                                            *target_id == my_id_clone
                                                        },
                                                        ServerMessage::RemoteControlAllowed { requester_id, target_id, allowed } => {
                                                            // Deliver to the requester always so they see
                                                            // grant/deny outcome. Also deliver to the
                                                            // target on a successful grant so the
                                                            // controlled-party banner can reactively
                                                            // appear (and on a server-side rejection,
                                                            // so the target's modal-dismissed state stays
                                                            // consistent — we still skip in that case
                                                            // since the target never had `controlling_peer`
                                                            // set yet).
                                                            *requester_id == my_id_clone
                                                                || (*allowed && *target_id == my_id_clone)
                                                        },
                                                        ServerMessage::RemoteControlStopped { sender_id, peer_id } => {
                                                            // Targeted delivery: the peer being controlled
                                                            // and the controller both need to know the
                                                            // session ended.
                                                            *sender_id == my_id_clone || *peer_id == my_id_clone
                                                        },
                                                        ServerMessage::RemoteControlAction { target_id, .. } => {
                                                            *target_id == my_id_clone
                                                        },
                                                        ServerMessage::FaceExpression { sender_id, .. } => {
                                                            // Scope to the same breakout room as the sender,
                                                            // consistent with PeerSpeaking / Transcription.
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            let source_loc = locs.get(sender_id).cloned().flatten();
                                                            my_loc == source_loc
                                                        },
                                                        ServerMessage::Transcription { user_id, .. } => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            let source_loc = locs.get(user_id).cloned().flatten();
                                                            my_loc == source_loc
                                                        },
                                                        ServerMessage::E2EEKeyExchange { from_id, .. } => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            let source_loc = locs.get(from_id).cloned().flatten();
                                                            my_loc == source_loc
                                                        },
                                                        ServerMessage::PeerSpeaking { user_id, speaking } => {
                                                            // Always deliver speaking=false so peers
                                                            // clear stale indicators even if the
                                                            // speaker moved to a different room.
                                                            if !speaking {
                                                                true
                                                            } else {
                                                                let locs = locations_clone.lock().unwrap();
                                                                let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                                let source_loc = locs.get(user_id).cloned().flatten();
                                                                my_loc == source_loc
                                                            }
                                                        },
                                                        _ => true,
                                                    };

                                                    if should_send
                                                        && forward_tx.send(msg).await.is_err() { break; }
                                                },
                                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                                    // We missed broadcasts, possibly whiteboard
                                                    // draws/evictions. A count-only eviction
                                                    // notice cannot repair that, so resend an
                                                    // authoritative snapshot; the client
                                                    // replaces its history and repaints. The
                                                    // client also drops strokes it already has
                                                    // (by revision), so any draws replayed from
                                                    // the buffer after this snapshot are safe.
                                                    // This cannot loop unboundedly: sending the
                                                    // snapshot awaits `forward_tx`, so a socket
                                                    // that is not draining blocks here instead of
                                                    // hitting `Lagged` again.
                                                    let history: Vec<shared::DrawAction> = {
                                                        let wb = whiteboard_for_task.lock().unwrap();
                                                        wb.clone()
                                                    };
                                                    if forward_tx
                                                        .send(ServerMessage::WhiteboardHistory(history))
                                                        .await
                                                        .is_err()
                                                    {
                                                        break;
                                                    }
                                                }
                                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                            }
                                        }
                                    }));

                                    let _ = tx.send(ServerMessage::ParticipantJoined(me));

                                    let current_list: Vec<Participant> = {
                                        let participants = participants_mutex.lock().unwrap();
                                        let locs = participant_locations_mutex.lock().unwrap();
                                        participants.values().filter(|p| {
                                            locs.get(&p.id).cloned().flatten().is_none()
                                        }).cloned().collect()
                                    };
                                    let _ = internal_tx.send(ServerMessage::ParticipantList(current_list)).await;

                                    let knocking_list: Vec<Participant> = {
                                        let knocking = knocking_mutex.lock().unwrap();
                                        knocking.values().map(|(p, _)| p.clone()).collect()
                                    };
                                    for p in knocking_list {
                                        let _ = internal_tx.send(ServerMessage::KnockingParticipant(p)).await;
                                    }

                                    let history: Vec<shared::DrawAction> = {
                                        let wb = whiteboard_mutex.lock().unwrap();
                                        wb.clone()
                                    };
                                    if !history.is_empty() {
                                        let _ = internal_tx.send(ServerMessage::WhiteboardHistory(history)).await;
                                    }

                                    // Send Existing Polls
                                    let polls_list: Vec<shared::Poll> = {
                                        let polls = polls_mutex.lock().unwrap();
                                        polls.values().cloned().collect()
                                    };
                                    if !polls_list.is_empty() {
                                        let _ = internal_tx.send(ServerMessage::PollsList(polls_list)).await;
                                    }

                                    // Send Shared Video State
                                    let shared_url = {
                                        shared_video_mutex.lock().unwrap().clone()
                                    };
                                    if let Some(url) = shared_url {
                                        let _ = internal_tx.send(ServerMessage::VideoShared(url)).await;
                                    }
                                },
                                ClientMessage::Chat { content, recipient_id, attachment, room_id } => {
                                    if let Some(uid) = &my_id {
                                        let is_visitor = {
                                            participants_mutex.lock().unwrap().get(uid).map(|p| p.is_visitor).unwrap_or(false)
                                        };
                                        if is_visitor {
                                            let _ = internal_tx.send(ServerMessage::Error("Visitors cannot send chat messages".to_string())).await;
                                            continue;
                                        }

                                        // Security: Only allow the client to send a chat message if the room_id they provided
                                        // matches the room_id the server believes they are currently in.
                                        let is_authorized = room_id == my_room_id;

                                        if !is_authorized {
                                            let _ = internal_tx.send(ServerMessage::Error("Unauthorized: Cannot send message to a different room".to_string())).await;
                                            continue;
                                        }

                                        let res = chat::process_chat_message(
                                            uid,
                                            &room_id,
                                            content,
                                            recipient_id,
                                            attachment,
                                            &state
                                        );
                                        match res {
                                            Ok(msg) => {
                                                let _ = tx.send(msg);
                                            },
                                            Err(e) => {
                                                let _ = internal_tx.send(ServerMessage::Error(e)).await;
                                            }
                                        }
                                    }
                                },
                                ClientMessage::SetCameraMuteStatus(muted) => {
                                    if let Some(uid) = &my_id {
                                        let updated_participant = {
                                            let mut participants = participants_mutex.lock().unwrap();
                                            if let Some(p) = participants.get_mut(uid) {
                                                p.is_camera_muted = muted;
                                                Some(p.clone())
                                            } else {
                                                None
                                            }
                                        };
                                        if let Some(p) = updated_participant {
                                            let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                        }
                                    }
                                },
                                ClientMessage::ToggleRoomLock(password) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let new_config = {
                                                let mut config = room_config_mutex.lock().unwrap();
                                                config.is_locked = !config.is_locked;
                                                if config.is_locked {
                                                    // Locking: store (or clear) the password.
                                                    let pw = password
                                                        .filter(|p| !p.trim().is_empty());
                                                    config.access_password = pw;
                                                    config.has_password = config.access_password.is_some();
                                                } else {
                                                    // Unlocking always clears any stored password.
                                                    config.access_password = None;
                                                    config.has_password = false;
                                                }
                                                config.clone()
                                            };
                                            let _ = tx.send(ServerMessage::RoomUpdated(new_config));
                                        }
                                    }
                                },
                                ClientMessage::ToggleRecording => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let new_config = {
                                                let mut config = room_config_mutex.lock().unwrap();
                                                config.is_recording = !config.is_recording;
                                                config.clone()
                                            };
                                            let _ = tx.send(ServerMessage::RoomUpdated(new_config));
                                        }
                                    }
                                },
                                ClientMessage::CreatePoll(poll) => {
                                    if let Some(uid) = &my_id {
                                        let is_visitor = {
                                            participants_mutex.lock().unwrap().get(uid).map(|p| p.is_visitor).unwrap_or(false)
                                        };
                                        if is_visitor {
                                            let _ = internal_tx.send(ServerMessage::Error("Visitors cannot create polls".to_string())).await;
                                            continue;
                                        }
                                        match polls::create_poll(uid, poll, &state) {
                                            Ok(outcome) => {
                                                // Tell clients about evicted polls before the
                                                // new one so their list stays consistent.
                                                if !outcome.removed.is_empty() {
                                                    let _ = tx.send(ServerMessage::PollRemoved(outcome.removed));
                                                }
                                                let _ = tx.send(outcome.message);
                                            },
                                            Err(e) => { let _ = internal_tx.send(ServerMessage::Error(e)).await; }
                                        }
                                    }
                                },
                                ClientMessage::Vote { poll_id, option_id } => {
                                    if let Some(uid) = &my_id {
                                        let is_visitor = {
                                            participants_mutex.lock().unwrap().get(uid).map(|p| p.is_visitor).unwrap_or(false)
                                        };
                                        if is_visitor {
                                            let _ = internal_tx.send(ServerMessage::Error("Visitors cannot vote".to_string())).await;
                                            continue;
                                        }
                                        match polls::vote(uid, poll_id, option_id, &state) {
                                            Ok(msg) => { let _ = tx.send(msg); },
                                            Err(e) => { let _ = internal_tx.send(ServerMessage::Error(e)).await; }
                                        }
                                    }
                                },
                                ClientMessage::ClosePoll(poll_id) => {
                                    if let Some(uid) = &my_id {
                                        match polls::close_poll(uid, poll_id, &state) {
                                            Ok(msg) => { let _ = tx.send(msg); },
                                            Err(e) => { let _ = internal_tx.send(ServerMessage::Error(e)).await; }
                                        }
                                    }
                                },
                                ClientMessage::Draw(action) => {
                                    if let Some(uid) = &my_id {
                                        let is_visitor = {
                                            participants_mutex.lock().unwrap().get(uid).map(|p| p.is_visitor).unwrap_or(false)
                                        };
                                        if is_visitor {
                                            let _ = internal_tx.send(ServerMessage::Error("Visitors cannot draw on the whiteboard".to_string())).await;
                                            continue;
                                        }
                                        // Mutation + broadcast happen atomically
                                        // under the whiteboard lock (see
                                        // `apply_draw`), so concurrent drawers
                                        // cannot reach clients out of order.
                                        whiteboard::apply_draw(uid, action, &state);
                                    }
                                },
                                ClientMessage::Reaction(emoji) => {
                                    if let Some(uid) = &my_id {
                                        let is_visitor = {
                                            participants_mutex.lock().unwrap().get(uid).map(|p| p.is_visitor).unwrap_or(false)
                                        };
                                        if is_visitor {
                                            let _ = internal_tx.send(ServerMessage::Error("Visitors cannot send reactions".to_string())).await;
                                            continue;
                                        }
                                        let _ = tx.send(ServerMessage::Reaction {
                                            sender_id: uid.clone(),
                                            emoji,
                                        });
                                    }
                                },
                                ClientMessage::UpdateProfile(new_name) => {
                                    if let Some(uid) = &my_id {
                                        // Same cap as `Join`: reject over-long renames
                                        // rather than storing/broadcasting them.
                                        if new_name.chars().count() > shared::MAX_PARTICIPANT_NAME_LEN {
                                            let _ = internal_tx.send(ServerMessage::Error("Name too long".to_string())).await;
                                            continue;
                                        }
                                        let updated_participant = {
                                            let mut participants = participants_mutex.lock().unwrap();
                                            if let Some(p) = participants.get_mut(uid) {
                                                p.name = new_name.clone();
                                                Some(p.clone())
                                            } else {
                                                None
                                            }
                                        };

                                        if let Some(p) = updated_participant {
                                            let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                        }
                                    }
                                },
                                ClientMessage::ToggleScreenShare => {
                                    if let Some(uid) = &my_id {
                                        let is_visitor = {
                                            participants_mutex.lock().unwrap().get(uid).map(|p| p.is_visitor).unwrap_or(false)
                                        };
                                        if is_visitor {
                                            let _ = internal_tx.send(ServerMessage::Error("Visitors cannot share screen".to_string())).await;
                                            continue;
                                        }
                                        let updated_participant = {
                                            let mut participants = participants_mutex.lock().unwrap();
                                            if let Some(p) = participants.get_mut(uid) {
                                                p.is_sharing_screen = !p.is_sharing_screen;
                                                Some(p.clone())
                                            } else {
                                                None
                                            }
                                        };

                                        if let Some(p) = updated_participant {
                                            let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                        }
                                    }
                                },
                                ClientMessage::ToggleRaiseHand => {
                                    if let Some(uid) = &my_id {
                                        let is_visitor = {
                                            participants_mutex.lock().unwrap().get(uid).map(|p| p.is_visitor).unwrap_or(false)
                                        };
                                        if is_visitor {
                                            let _ = internal_tx.send(ServerMessage::Error("Visitors cannot raise hand".to_string())).await;
                                            continue;
                                        }
                                        let updated_participant = {
                                            let mut participants = participants_mutex.lock().unwrap();
                                            if let Some(p) = participants.get_mut(uid) {
                                                p.is_hand_raised = !p.is_hand_raised;
                                                if p.is_hand_raised {
                                                    p.hand_raised_at = Some(chrono::Utc::now().timestamp_millis() as u64);
                                                } else {
                                                    p.hand_raised_at = None;
                                                }
                                                Some(p.clone())
                                            } else {
                                                None
                                            }
                                        };

                                        if let Some(p) = updated_participant {
                                            let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                        }
                                    }
                                },
                                ClientMessage::Typing(is_typing) => {
                                    if let Some(uid) = &my_id {
                                        let _ = tx.send(ServerMessage::PeerTyping {
                                            user_id: uid.clone(),
                                            is_typing,
                                            room_id: my_room_id.clone(),
                                        });
                                    }
                                },
                                ClientMessage::CreateBreakoutRoom(name) => {
                                    if let Some(uid) = &my_id {
                                        match breakout::create_breakout_room(uid, name, &state) {
                                            Ok(msg) => { let _ = tx.send(msg); },
                                            Err(e) => { let _ = internal_tx.send(ServerMessage::Error(e)).await; }
                                        }
                                    }
                                },
                                ClientMessage::CloseAllBreakoutRooms => {
                                    if let Some(uid) = &my_id {
                                        match breakout::close_all_breakout_rooms(uid, &state) {
                                            Ok(msgs) => { for m in msgs { let _ = tx.send(m); } },
                                            Err(e) => { let _ = internal_tx.send(ServerMessage::Error(e)).await; }
                                        }
                                    }
                                },
                                ClientMessage::MoveParticipantToRoom { target_id, room_id } => {
                                    if let Some(uid) = &my_id {
                                        match breakout::move_participant_to_room(uid, target_id, room_id, &state) {
                                            Ok(msgs) => { for m in msgs { let _ = tx.send(m); } },
                                            Err(e) => { let _ = internal_tx.send(ServerMessage::Error(e)).await; }
                                        }
                                    }
                                },
                                ClientMessage::AutoAssignToBreakoutRooms => {
                                    if let Some(uid) = &my_id {
                                        match breakout::auto_assign_participants(uid, &state) {
                                            Ok(msgs) => { for m in msgs { let _ = tx.send(m); } },
                                            Err(e) => { let _ = internal_tx.send(ServerMessage::Error(e)).await; }
                                        }
                                    }
                                },
                                ClientMessage::JoinBreakoutRoom(room_id) => {
                                    if let Some(uid) = &my_id {
                                        // Pre-validate room existence to prevent false ParticipantLeft broadcasts
                                        let is_valid = match &room_id {
                                            Some(rid) => state.breakout_rooms.lock().unwrap().contains_key(rid),
                                            None => true,
                                        };

                                        if !is_valid {
                                            let _ = internal_tx.send(ServerMessage::Error("Breakout room not found".to_string())).await;
                                            continue;
                                        }

                                        let me = {
                                            let participants = participants_mutex.lock().unwrap();
                                            participants.get(uid).cloned()
                                        };

                                        // Capture current location to embed in the leave message
                                        let old_room = {
                                            let locations = participant_locations_mutex.lock().unwrap();
                                            locations.get(uid).cloned().flatten()
                                        };

                                        // Broadcast leave using the embedded location, immune to async races
                                        let _ = tx.send(ServerMessage::ParticipantLeft { id: uid.clone(), room_id: old_room });

                                        match breakout::join_breakout_room(uid, room_id, &state) {
                                            Ok((new_rid, msgs)) => {
                                                my_room_id = new_rid;
                                                for msg in msgs {
                                                    let _ = internal_tx.send(msg).await;
                                                }

                                                // Broadcast join to new room (after location update)
                                                if let Some(p) = me {
                                                    let _ = tx.send(ServerMessage::ParticipantJoined(p));
                                                }
                                            },
                                            Err(e) => {
                                                // Should not be hit due to pre-validation, but included for safety
                                                let _ = internal_tx.send(ServerMessage::Error(e)).await;
                                            }
                                        }
                                    }
                                },
                                ClientMessage::StartShareVideo(url) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            {
                                                let mut v = shared_video_mutex.lock().unwrap();
                                                *v = Some(url.clone());
                                            }
                                            let _ = tx.send(ServerMessage::VideoShared(url));
                                        }
                                    }
                                },
                                ClientMessage::StopShareVideo => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            {
                                                let mut v = shared_video_mutex.lock().unwrap();
                                                *v = None;
                                            }
                                            let _ = tx.send(ServerMessage::VideoStopped);
                                        }
                                    }
                                },
                                ClientMessage::RequestUnmutePermission => {
                                    if let Some(uid) = &my_id {
                                        let msgs = moderation::handle_unmute_permission_request(uid, &state);
                                        for m in msgs { let _ = tx.send(m); }
                                    }
                                },
                                ClientMessage::RequestCameraPermission => {
                                    if let Some(uid) = &my_id {
                                        let msgs = moderation::handle_camera_permission_request(uid, &state);
                                        for m in msgs { let _ = tx.send(m); }
                                    }
                                },
                                ClientMessage::GrantUnmutePermission(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let msgs = moderation::grant_unmute_permission(uid, &target_id, &state);
                                        for m in msgs { let _ = tx.send(m); }
                                    }
                                },
                                ClientMessage::GrantCameraPermission(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let msgs = moderation::grant_camera_permission(uid, &target_id, &state);
                                        for m in msgs { let _ = tx.send(m); }
                                    }
                                },
                                ClientMessage::Speaking(is_speaking) => {
                                    if let Some(uid) = &my_id {
                                        // Enforce AV Moderation
                                        if is_speaking {
                                            let (moderation_enabled, has_permission) = {
                                                let config = room_config_mutex.lock().unwrap();
                                                let permissions = state.unmute_permissions.lock().unwrap();
                                                (config.audio_moderation_enabled, permissions.contains(uid))
                                            };
                                            if moderation_enabled && !has_permission {
                                                let _ = internal_tx.send(ServerMessage::Error("Audio is moderated. Request permission to speak.".to_string())).await;
                                                continue;
                                            }
                                        }

                                        let mut update_stats = false;
                                        if is_speaking {
                                            {
                                                let mut starts = speaking_start_times_mutex.lock().unwrap();
                                                starts.insert(uid.clone(), chrono::Utc::now().timestamp_millis() as u64);
                                            }

                                            // Transcription logic (Mocked)
                                            let is_subtitles_enabled = {
                                                room_config_mutex.lock().unwrap().is_subtitles_enabled
                                            };
                                            if is_subtitles_enabled {
                                                let text = format!("{} is speaking...", {
                                                    let p_map = participants_mutex.lock().unwrap();
                                                    p_map.get(uid).map(|p| p.name.clone()).unwrap_or_else(|| "Unknown".to_string())
                                                });
                                                let _ = tx.send(ServerMessage::Transcription {
                                                    user_id: uid.clone(),
                                                    text,
                                                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                                                });
                                            }
                                        } else {
                                            let start_opt = {
                                                let mut starts = speaking_start_times_mutex.lock().unwrap();
                                                starts.remove(uid)
                                            };
                                            if let Some(start) = start_opt {
                                                let now = chrono::Utc::now().timestamp_millis() as u64;
                                                if now > start {
                                                    let delta = now - start;
                                                    let mut participants = participants_mutex.lock().unwrap();
                                                    if let Some(p) = participants.get_mut(uid) {
                                                        p.speaking_time += delta;
                                                        update_stats = true;
                                                    }
                                                }
                                            }
                                        }

                                        if update_stats {
                                            let updated_p = {
                                                let participants = participants_mutex.lock().unwrap();
                                                participants.get(uid).cloned()
                                            };
                                            if let Some(p) = updated_p {
                                                let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                            }
                                        }

                                        let _ = tx.send(ServerMessage::PeerSpeaking {
                                            user_id: uid.clone(),
                                            speaking: is_speaking,
                                        });
                                    }
                                },
                                ClientMessage::SetMuteStatus(muted) => {
                                    if let Some(uid) = &my_id {
                                        let updated_participant = {
                                            let mut participants = participants_mutex.lock().unwrap();
                                            if let Some(p) = participants.get_mut(uid) {
                                                p.is_muted = muted;
                                                Some(p.clone())
                                            } else {
                                                None
                                            }
                                        };
                                        if let Some(p) = updated_participant {
                                            let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                        }
                                    }
                                },
                                ClientMessage::Authenticate { username, password } => {
                                    if let Some(_uid) = &my_id {
                                        // FIXME: mock auth — accepts credentials from env vars MOCK_AUTH_USER / MOCK_AUTH_PASS (defaults: admin / admin123). Replace with real authentication before using is_authenticated to gate features.
                                        let expected_user = std::env::var("MOCK_AUTH_USER").unwrap_or_else(|_| "admin".to_string());
                                        let expected_pass = std::env::var("MOCK_AUTH_PASS").unwrap_or_else(|_| "admin123".to_string());
                                        if username == expected_user && password.as_deref() == Some(expected_pass.as_str()) {
                                            let _ = internal_tx.send(ServerMessage::AuthenticationResult(true)).await;
                                        } else {
                                            let _ = internal_tx.send(ServerMessage::AuthenticationResult(false)).await;
                                        }
                                    }
                                },
                                ClientMessage::FetchCalendar => {
                                    if let Some(_uid) = &my_id {
                                        let msg = calendar::handle_fetch_calendar();
                                        let _ = internal_tx.send(msg).await;
                                    }
                                },
                                ClientMessage::AnalyticsEvent { name, properties } => {
                                    if let Some(uid) = &my_id {
                                        // Rate limit: max 10 analytics events per second per connection
                                        let now = std::time::Instant::now();
                                        if now.duration_since(analytics_window_start) >= std::time::Duration::from_secs(1) {
                                            analytics_count = 0;
                                            analytics_window_start = now;
                                        }
                                        analytics_count += 1;
                                        if analytics_count > 10 {
                                            continue;
                                        }
                                        let safe_name: String = name.chars().take(200).filter(|c| !c.is_control()).collect();
                                        let safe_props: String = properties.chars().take(1000).filter(|c| !c.is_control()).collect();
                                        tracing::info!(target: "analytics", user_id = %uid, event = %safe_name, properties = %safe_props, "Received Analytics Event");
                                    }
                                },
                                ClientMessage::MuteParticipant(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let same_room = {
                                                let locs = participant_locations_mutex.lock().unwrap();
                                                let host_loc = locs.get(uid).cloned().flatten();
                                                let target_loc = locs.get(&target_id).cloned().flatten();
                                                host_loc == target_loc
                                            };
                                            if !same_room {
                                                continue;
                                            }
                                            let updated_p = {
                                                let mut participants = participants_mutex.lock().unwrap();
                                                if let Some(p) = participants.get_mut(&target_id) {
                                                    p.is_muted = true;
                                                    Some(p.clone())
                                                } else {
                                                    None
                                                }
                                            };
                                            if let Some(p) = updated_p {
                                                let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                                let _ = tx.send(ServerMessage::MutedByHost(target_id));
                                            }
                                        }
                                    }
                                },
                                ClientMessage::TransferHost(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let is_host = {
                                            room_config_mutex.lock().unwrap().host_id == Some(uid.clone())
                                        };
                                        if is_host {
                                            let target_exists = {
                                                let participants = participants_mutex.lock().unwrap();
                                                participants.contains_key(&target_id)
                                            };
                                            if target_exists {
                                                // Auto-promote visitor when transferring host to them
                                                let promoted = {
                                                    let mut participants = participants_mutex.lock().unwrap();
                                                    if let Some(p) = participants.get_mut(&target_id) {
                                                        if p.is_visitor {
                                                            p.is_visitor = false;
                                                            Some(p.clone())
                                                        } else {
                                                            None
                                                        }
                                                    } else {
                                                        None
                                                    }
                                                };
                                                if let Some(p) = promoted {
                                                    let _ = tx.send(ServerMessage::ParticipantUpdated(p));
                                                    let _ = tx.send(ServerMessage::VisitorPromoted(target_id.clone()));
                                                }
                                                let new_config = {
                                                    let mut config = room_config_mutex.lock().unwrap();
                                                    config.host_id = Some(target_id);
                                                    config.clone()
                                                };
                                                let _ = tx.send(ServerMessage::RoomUpdated(new_config));
                                            }
                                        }
                                    }
                                },
                                ClientMessage::Ping => {
                                    let _ = internal_tx.send(ServerMessage::Pong { timestamp: chrono::Utc::now().timestamp_millis() as u64 }).await;
                                },
                                ClientMessage::SetAudioOnly(enabled) => {
                                    if let Some(uid) = &my_id {
                                        let _ = tx.send(ServerMessage::AudioOnlyChanged {
                                            user_id: uid.clone(),
                                            enabled,
                                        });
                                    }
                                },
                                ClientMessage::FlipLocalVideo(_enabled) => {
                                },
                                ClientMessage::PinParticipant(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let _ = tx.send(ServerMessage::ParticipantPinned {
                                            user_id: uid.clone(),
                                            target_id,
                                        });
                                    }
                                },
                                ClientMessage::SetParticipantVolume { target_id, volume } => {
                                    if let Some(uid) = &my_id {
                                        let _ = tx.send(ServerMessage::ParticipantVolumeChanged {
                                            user_id: uid.clone(),
                                            target_id,
                                            volume,
                                        });
                                    }
                                },
                                ClientMessage::MuteEveryoneElse(target_id) => {
                                    if let Some(uid) = &my_id {
                                        let msgs = moderation::mute_everyone_else(uid, &target_id, &state);
                                        for m in msgs {
                                            let _ = tx.send(m);
                                        }
                                    }
                                },
                                ClientMessage::MuteAll => {
                                    if let Some(uid) = &my_id {
                                        let msgs = moderation::mute_all(uid, &state);
                                        for msg in msgs {
                                            let _ = tx.send(msg);
                                        }
                                    }
                                },
                                ClientMessage::Offer { target_id, sdp } => {
                                    if let Some(uid) = &my_id {
                                        let _ = tx.send(ServerMessage::Offer {
                                            source_id: uid.clone(),
                                            target_id,
                                            sdp,
                                        });
                                    }
                                },
                                ClientMessage::Answer { target_id, sdp } => {
                                    if let Some(uid) = &my_id {
                                        let _ = tx.send(ServerMessage::Answer {
                                            source_id: uid.clone(),
                                            target_id,
                                            sdp,
                                        });
                                    }
                                },
                                ClientMessage::IceCandidate { target_id, candidate, sdp_mid, sdp_m_line_index } => {
                                    if let Some(uid) = &my_id {
                                        let _ = tx.send(ServerMessage::IceCandidate {
                                            source_id: uid.clone(),
                                            target_id,
                                            candidate,
                                            sdp_mid,
                                            sdp_m_line_index,
                                        });
                                    }
                                },
                                ClientMessage::RequestRemoteControl(_)
                                | ClientMessage::GrantRemoteControl(_)
                                | ClientMessage::DenyRemoteControl(_)
                                | ClientMessage::StopRemoteControl(_)
                                | ClientMessage::RemoteControlAction { .. } => {
                                    if let Some(uid) = &my_id {
                                        // Rate limit RemoteControlAction (high-frequency,
                                        // driven by mouse/keyboard events) to protect the
                                        // broadcast channel even if the client throttle is
                                        // bypassed. Lifecycle messages (request/grant/deny/
                                        // stop) are infrequent and not rate-limited here.
                                        if matches!(client_msg, ClientMessage::RemoteControlAction { .. }) {
                                            let now = std::time::Instant::now();
                                            if now.duration_since(rc_window_start) >= std::time::Duration::from_secs(1) {
                                                rc_count = 0;
                                                rc_window_start = now;
                                            }
                                            rc_count += 1;
                                            if rc_count > 30 {
                                                continue;
                                            }
                                        }
                                        let msgs = remote_control::handle_remote_control(uid, client_msg, &state);
                                        for m in msgs {
                                            let _ = tx.send(m);
                                        }
                                    }
                                }
                            }
                        }
                    },
                    _ => break, // Disconnect or Error
                }
            },
            // 2. Control Messages (Lobby Decision)
            Some(granted) = control_rx.recv() => {
                if granted {
                    if let Some(id) = knocking_id.take() {
                        let me_opt = {
                            let mut knocking = knocking_mutex.lock().unwrap();
                            knocking.remove(&id).map(|(p, _)| p)
                        };

                        if let Some(me) = me_opt {
                            let (joined, new_host_assigned) = {
                                let mut participants = participants_mutex.lock().unwrap();
                                let mut config = room_config_mutex.lock().unwrap();
                                if participants.len() >= config.max_participants as usize {
                                    (false, false)
                                } else {
                                    // Robust Host Check: Clear host_id if participant no longer exists
                                    if let Some(hid) = &config.host_id
                                        && !participants.contains_key(hid) {
                                            config.host_id = None;
                                        }
                                    let assigned = if config.host_id.is_none() && !me.is_visitor {
                                        config.host_id = Some(id.clone());
                                        true
                                    } else {
                                        false
                                    };
                                    participants.insert(id.clone(), me.clone());
                                    (true, assigned)
                                }
                            };

                            if !joined {
                                let _ = internal_tx.send(ServerMessage::Error("Room is full".to_string())).await;
                                continue;
                            }
                            my_id = Some(id.clone());

                            let _ = internal_tx.send(ServerMessage::Welcome { id: id.clone() }).await;

                            // Send Chat History
                            let history: Vec<shared::ChatMessage> = {
                                let history = chat_history_mutex.lock().unwrap();
                                history.clone()
                            };
                            if !history.is_empty() {
                                let _ = internal_tx.send(ServerMessage::ChatHistory(history)).await;
                            }

                            let current_config = {
                                room_config_mutex.lock().unwrap().clone()
                            };
                            if new_host_assigned {
                                let _ = tx.send(ServerMessage::RoomUpdated(current_config.clone()));
                            }
                            let _ = internal_tx.send(ServerMessage::RoomUpdated(current_config)).await;

                             // Register initial location (Main Room)
                            {
                                let mut locations = participant_locations_mutex.lock().unwrap();
                                locations.insert(id.clone(), None);
                            }

                            // Subscribe to broadcast
                            let mut rx = tx.subscribe();
                            let forward_tx = internal_tx.clone();
                            let my_id_clone = id.clone();
                            let room_config_clone = room_config_mutex.clone();
                            let locations_clone = participant_locations_mutex.clone();
                            let whiteboard_for_task = whiteboard_mutex.clone();
                            broadcast_task = Some(tokio::spawn(async move {
                                loop {
                                    match rx.recv().await {
                                        Ok(msg) => {
                                            // Filter based on room and recipient
                                            let should_send = match &msg {
                                                ServerMessage::Chat { message, room_id } => {
                                                    let my_loc = {
                                                        let locs = locations_clone.lock().unwrap();
                                                        locs.get(&my_id_clone).cloned().flatten()
                                                    };
                                                    if *room_id != my_loc {
                                                        false
                                                    } else if let Some(target) = &message.recipient_id {
                                                        *target == my_id_clone || message.user_id == my_id_clone // Must echo private message back to self
                                                    } else {
                                                        true
                                                    }
                                                },
                                                ServerMessage::PeerTyping { room_id, .. } => {
                                                    let my_loc = {
                                                        let locs = locations_clone.lock().unwrap();
                                                        locs.get(&my_id_clone).cloned().flatten()
                                                    };
                                                    *room_id == my_loc
                                                },
                                                ServerMessage::Offer { source_id, target_id, .. }
                                                | ServerMessage::Answer { source_id, target_id, .. }
                                                | ServerMessage::IceCandidate { source_id, target_id, .. } => {
                                                    if *target_id == my_id_clone {
                                                        let locs = locations_clone.lock().unwrap();
                                                        let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                        let source_loc = locs.get(source_id).cloned().flatten();
                                                        my_loc == source_loc
                                                    } else {
                                                        false
                                                    }
                                                },
                                                        ServerMessage::ParticipantJoined(p) => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            let source_loc = locs.get(&p.id).cloned().flatten();
                                                            my_loc == source_loc
                                                        },
                                                        ServerMessage::ParticipantUpdated(p) => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            let source_loc = locs.get(&p.id).cloned().flatten();
                                                            my_loc == source_loc || p.id == my_id_clone
                                                        },
                                                        ServerMessage::ParticipantLeft { id, room_id } => {
                                                            if *id == my_id_clone {
                                                                false
                                                            } else {
                                                                let locs = locations_clone.lock().unwrap();
                                                                let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                                // Use the room_id embedded in the message instead of looking it up
                                                                my_loc == *room_id
                                                            }
                                                        },
                                                ServerMessage::Kicked { target_id: id, room_id } => {
                                                    let locs = locations_clone.lock().unwrap();
                                                    let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                    my_loc == *room_id || *id == my_id_clone
                                                },
                                                ServerMessage::ForcedMoveToRoom { target_id, .. } => {
                                                    *target_id == my_id_clone
                                                },
                                                ServerMessage::MutedByHost(id) | ServerMessage::CameraMutedByHost(id) => {
                                                            let locs = locations_clone.lock().unwrap();
                                                            let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                            let source_loc = locs.get(id).cloned().flatten();
                                                            my_loc == source_loc || *id == my_id_clone
                                                        },
                                                ServerMessage::PowerStatusUpdated { user_id, .. } => {
                                                    let locs = locations_clone.lock().unwrap();
                                                    let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                    let source_loc = locs.get(user_id).cloned().flatten();
                                                    my_loc == source_loc
                                                },
                                                ServerMessage::E2EEKeyExchange { from_id, .. } => {
                                                    let locs = locations_clone.lock().unwrap();
                                                    let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                    let source_loc = locs.get(from_id).cloned().flatten();
                                                    my_loc == source_loc
                                                },
                                                ServerMessage::RecordingStatusChanged { user_id, .. } => {
                                                    let locs = locations_clone.lock().unwrap();
                                                    let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                    let source_loc = locs.get(user_id).cloned().flatten();
                                                    my_loc == source_loc
                                                },
                                                ServerMessage::UnmuteRequested { target_id, .. } => {
                                                    *target_id == my_id_clone
                                                },
                                                        ServerMessage::UnmutePermissionRequested { .. } => {
                                                            let config = room_config_clone.lock().unwrap();
                                                            config.host_id == Some(my_id_clone.clone())
                                                        },
                                                        ServerMessage::CameraPermissionRequested { .. } => {
                                                            let config = room_config_clone.lock().unwrap();
                                                            config.host_id == Some(my_id_clone.clone())
                                                        },
                                                        ServerMessage::PermissionGranted { target_id, .. } => {
                                                            *target_id == my_id_clone
                                                        },
                                                ServerMessage::RemoteControlRequest { target_id, .. } => {
                                                    *target_id == my_id_clone
                                                },
                                                ServerMessage::RemoteControlAllowed { requester_id, target_id, allowed } => {
                                                    // Deliver to the requester always; also to the target
                                                    // on a successful grant so their controlled-party
                                                    // banner appears reactively. See the equivalent arm
                                                    // in the direct-join broadcast filter for context.
                                                    *requester_id == my_id_clone
                                                        || (*allowed && *target_id == my_id_clone)
                                                },
                                                ServerMessage::RemoteControlStopped { sender_id, peer_id } => {
                                                    *sender_id == my_id_clone || *peer_id == my_id_clone
                                                },
                                                ServerMessage::RemoteControlAction { target_id, .. } => {
                                                    *target_id == my_id_clone
                                                },
                                                ServerMessage::FaceExpression { sender_id, .. } => {
                                                    let locs = locations_clone.lock().unwrap();
                                                    let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                    let source_loc = locs.get(sender_id).cloned().flatten();
                                                    my_loc == source_loc
                                                },
                                                ServerMessage::Transcription { user_id, .. } => {
                                                    let locs = locations_clone.lock().unwrap();
                                                    let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                    let source_loc = locs.get(user_id).cloned().flatten();
                                                    my_loc == source_loc
                                                },
                                                // Note: EtherpadUrlUpdated and GiphyShared are
                                                // currently never broadcast by the server (etherpad
                                                // changes go via RoomUpdated, GIFs via Chat). These
                                                // arms are kept for protocol completeness.
                                                ServerMessage::EtherpadUrlUpdated { room_id, .. } => {
                                                    let locs = locations_clone.lock().unwrap();
                                                    let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                    my_loc == *room_id
                                                },
                                                ServerMessage::GiphyShared { room_id, .. } => {
                                                    let locs = locations_clone.lock().unwrap();
                                                    let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                    my_loc == *room_id
                                                },
                                                ServerMessage::PeerSpeaking { user_id, speaking } => {
                                                    // Always deliver speaking=false so peers
                                                    // clear stale indicators even if the
                                                    // speaker moved to a different room.
                                                    if !speaking {
                                                        true
                                                    } else {
                                                        let locs = locations_clone.lock().unwrap();
                                                        let my_loc = locs.get(&my_id_clone).cloned().flatten();
                                                        let source_loc = locs.get(user_id).cloned().flatten();
                                                        my_loc == source_loc
                                                    }
                                                },
                                                _ => true,
                                            };

                                            if should_send
                                                && forward_tx.send(msg).await.is_err() { break; }
                                        },
                                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                            // Missed broadcasts may include whiteboard
                                            // draws/evictions, which a count-only notice
                                            // cannot repair. Resend an authoritative
                                            // snapshot so the client can replace its
                                            // history. Safe against replayed buffered
                                            // draws because the client drops strokes it
                                            // already holds (by revision).
                                            let history: Vec<shared::DrawAction> = {
                                                let wb = whiteboard_for_task.lock().unwrap();
                                                wb.clone()
                                            };
                                            if forward_tx
                                                .send(ServerMessage::WhiteboardHistory(history))
                                                .await
                                                .is_err()
                                            {
                                                break;
                                            }
                                        }
                                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                    }
                                }
                            }));

                            let _ = tx.send(ServerMessage::ParticipantJoined(me));

                            let current_list: Vec<Participant> = {
                                let participants = participants_mutex.lock().unwrap();
                                        let locs = participant_locations_mutex.lock().unwrap();
                                        participants.values().filter(|p| {
                                            locs.get(&p.id).cloned().flatten().is_none()
                                        }).cloned().collect()
                            };
                            let _ = internal_tx.send(ServerMessage::ParticipantList(current_list)).await;

                            let knocking_list: Vec<Participant> = {
                                let knocking = knocking_mutex.lock().unwrap();
                                knocking.values().map(|(p, _)| p.clone()).collect()
                            };
                            for p in knocking_list {
                                let _ = internal_tx.send(ServerMessage::KnockingParticipant(p)).await;
                            }

                            // Send Breakout Rooms List
                            let all_rooms: Vec<shared::BreakoutRoom> = {
                                let rooms = breakout_rooms_mutex.lock().unwrap();
                                rooms.values().cloned().collect()
                            };
                            if !all_rooms.is_empty() {
                                let _ = internal_tx.send(ServerMessage::BreakoutRoomsList(all_rooms)).await;
                            }

                            let history: Vec<shared::DrawAction> = {
                                let wb = whiteboard_mutex.lock().unwrap();
                                wb.clone()
                            };
                            if !history.is_empty() {
                                let _ = internal_tx.send(ServerMessage::WhiteboardHistory(history)).await;
                            }

                            // Send Existing Polls
                            let polls_list: Vec<shared::Poll> = {
                                let polls = polls_mutex.lock().unwrap();
                                polls.values().cloned().collect()
                            };
                            if !polls_list.is_empty() {
                                let _ = internal_tx.send(ServerMessage::PollsList(polls_list)).await;
                            }

                            // Send Shared Video State
                            let shared_url = {
                                shared_video_mutex.lock().unwrap().clone()
                            };
                            if let Some(url) = shared_url {
                                let _ = internal_tx.send(ServerMessage::VideoShared(url)).await;
                            }
                        } else {
                            let _ = internal_tx.send(ServerMessage::AccessDenied).await;
                        }
                    }
                } else {
                    knocking_id = None;
                    let _ = internal_tx.send(ServerMessage::AccessDenied).await;
                }
            }
        }
    }

    send_task.abort();

    // Cleanup
    if let Some(t) = broadcast_task {
        t.abort();
    }

    if let Some(id) = my_id {
        // Update speaking time before removal
        {
            let mut starts = speaking_start_times_mutex.lock().unwrap();
            if let Some(start) = starts.remove(&id) {
                let now = chrono::Utc::now().timestamp_millis() as u64;
                if now > start {
                    let delta = now - start;
                    let mut participants = participants_mutex.lock().unwrap();
                    if let Some(p) = participants.get_mut(&id) {
                        p.speaking_time += delta;
                        // Broadcast final update
                        let _ = tx.send(ServerMessage::ParticipantUpdated(p.clone()));
                    }
                }
            }
        }

        // Handle Host Leaving / Reassignment
        let new_host_assigned = {
            let mut participants = participants_mutex.lock().unwrap();
            participants.remove(&id);

            let mut config = room_config_mutex.lock().unwrap();
            if config.host_id == Some(id.clone()) {
                // Host left, assign new host if any participants remain
                if let Some(new_host) = participants
                    .values()
                    .find(|p| !p.is_visitor)
                    .map(|p| p.id.clone())
                {
                    config.host_id = Some(new_host.clone());
                    true
                } else {
                    config.host_id = None;
                    false
                }
            } else {
                false
            }
        };

        if new_host_assigned {
            let new_config = { room_config_mutex.lock().unwrap().clone() };
            let _ = tx.send(ServerMessage::RoomUpdated(new_config));
        }

        // Fetch location before cleanup to embed in message
        let old_room = {
            let locations = participant_locations_mutex.lock().unwrap();
            locations.get(&id).cloned().flatten()
        };

        let _ = tx.send(ServerMessage::ParticipantLeft {
            id: id.clone(),
            room_id: old_room,
        });

        // Cleanup location
        {
            let mut locations = participant_locations_mutex.lock().unwrap();
            locations.remove(&id);
        }
        // Clean up any remote-control sessions and pending requests
        // involving the disconnecting participant. Without this, sessions
        // leak across reconnections and could authorize a future client
        // (in the unlikely event of a UUID collision) to inject actions.
        {
            let mut sessions = state.remote_control_sessions.lock().unwrap();
            sessions.remove(&id);
            sessions.retain(|_, controlled| controlled != &id);
        }
        {
            let mut pending = state.pending_remote_control_requests.lock().unwrap();
            pending.retain(|(req, tgt)| req != &id && tgt != &id);
        }
        {
            let mut unmute = state.unmute_permissions.lock().unwrap();
            unmute.remove(&id);
        }
        {
            let mut camera = state.camera_permissions.lock().unwrap();
            camera.remove(&id);
        }
    } else if let Some(kid) = knocking_id {
        // If disconnected while knocking
        let removed = {
            let mut knocking = knocking_mutex.lock().unwrap();
            knocking.remove(&kid).is_some()
        };
        if removed {
            let _ = tx.send(ServerMessage::KnockingParticipantLeft(kid));
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn test_ws_handler() {
        let _ = true;
    }
}
