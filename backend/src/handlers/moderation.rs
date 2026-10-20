use crate::AppState;
use shared::ServerMessage;
use std::sync::Arc;

pub fn mute_all(sender_id: &str, state: &Arc<AppState>) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    let is_host = { state.room_config.lock().unwrap().host_id == Some(sender_id.to_string()) };

    if is_host {
        // Only mute participants in the same breakout room as the host.
        // This prevents a host in the main room from silently muting
        // participants in breakout rooms (or vice-versa).
        let host_location = {
            let locs = state.participant_locations.lock().unwrap();
            locs.get(sender_id).cloned().flatten()
        };

        let participants = {
            let p_map = state.participants.lock().unwrap();
            let locs = state.participant_locations.lock().unwrap();
            p_map
                .keys()
                .filter(|id| locs.get(*id).cloned().flatten() == host_location)
                .cloned()
                .collect::<Vec<String>>()
        };

        for target_id in participants {
            if target_id != sender_id {
                let updated_participant = {
                    let mut p_map = state.participants.lock().unwrap();
                    if let Some(p) = p_map.get_mut(&target_id) {
                        p.is_muted = true;
                        Some(p.clone())
                    } else {
                        None
                    }
                };
                if let Some(p) = updated_participant {
                    messages.push(ServerMessage::ParticipantUpdated(p));
                    messages.push(ServerMessage::MutedByHost(target_id));
                }
            }
        }
    }
    messages
}

pub fn mute_everyone_else(
    sender_id: &str,
    target_id: &str,
    state: &Arc<AppState>,
) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    let is_host = { state.room_config.lock().unwrap().host_id == Some(sender_id.to_string()) };

    if is_host {
        let host_location = {
            let locs = state.participant_locations.lock().unwrap();
            locs.get(sender_id).cloned().flatten()
        };

        let participants = {
            let p_map = state.participants.lock().unwrap();
            let locs = state.participant_locations.lock().unwrap();
            p_map
                .keys()
                .filter(|id| locs.get(*id).cloned().flatten() == host_location)
                .cloned()
                .collect::<Vec<String>>()
        };

        for pid in participants {
            if pid != sender_id && pid != target_id {
                let updated_participant = {
                    let mut p_map = state.participants.lock().unwrap();
                    if let Some(p) = p_map.get_mut(&pid) {
                        p.is_muted = true;
                        Some(p.clone())
                    } else {
                        None
                    }
                };
                if let Some(p) = updated_participant {
                    messages.push(ServerMessage::ParticipantUpdated(p));
                    messages.push(ServerMessage::MutedByHost(pid));
                }
            }
        }
    }
    messages
}

pub fn stop_screen_share_all(sender_id: &str, state: &Arc<AppState>) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    let is_host = { state.room_config.lock().unwrap().host_id == Some(sender_id.to_string()) };

    if is_host {
        let host_location = {
            let locs = state.participant_locations.lock().unwrap();
            locs.get(sender_id).cloned().flatten()
        };

        let participants = {
            let p_map = state.participants.lock().unwrap();
            let locs = state.participant_locations.lock().unwrap();
            p_map
                .values()
                .filter(|p| {
                    locs.get(&p.id).cloned().flatten() == host_location && p.is_sharing_screen
                })
                .cloned()
                .collect::<Vec<shared::Participant>>()
        };

        for mut p in participants {
            if p.id != sender_id {
                p.is_sharing_screen = false;
                {
                    let mut p_map = state.participants.lock().unwrap();
                    if let Some(participant) = p_map.get_mut(&p.id) {
                        participant.is_sharing_screen = false;
                    }
                }
                messages.push(ServerMessage::ParticipantUpdated(p));
                messages.push(ServerMessage::ScreenShareStoppedByHost);
            }
        }
    }
    messages
}

pub fn mute_camera_all(sender_id: &str, state: &Arc<AppState>) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    let is_host = { state.room_config.lock().unwrap().host_id == Some(sender_id.to_string()) };

    if is_host {
        let host_location = {
            let locs = state.participant_locations.lock().unwrap();
            locs.get(sender_id).cloned().flatten()
        };

        let participants = {
            let p_map = state.participants.lock().unwrap();
            let locs = state.participant_locations.lock().unwrap();
            p_map
                .keys()
                .filter(|id| locs.get(*id).cloned().flatten() == host_location)
                .cloned()
                .collect::<Vec<String>>()
        };

        for target_id in participants {
            if target_id != sender_id {
                let updated_participant = {
                    let mut p_map = state.participants.lock().unwrap();
                    if let Some(p) = p_map.get_mut(&target_id) {
                        p.is_camera_muted = true;
                        Some(p.clone())
                    } else {
                        None
                    }
                };
                if let Some(p) = updated_participant {
                    messages.push(ServerMessage::ParticipantUpdated(p));
                    messages.push(ServerMessage::CameraMutedByHost(target_id));
                }
            }
        }
    }
    messages
}

pub fn handle_unmute_permission_request(
    user_id: &str,
    state: &Arc<AppState>,
) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    let config = state.room_config.lock().unwrap();
    if config.audio_moderation_enabled {
        messages.push(ServerMessage::UnmutePermissionRequested {
            user_id: user_id.to_string(),
        });
    }
    messages
}

pub fn handle_camera_permission_request(
    user_id: &str,
    state: &Arc<AppState>,
) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    let config = state.room_config.lock().unwrap();
    if config.video_moderation_enabled {
        messages.push(ServerMessage::CameraPermissionRequested {
            user_id: user_id.to_string(),
        });
    }
    messages
}

pub fn grant_unmute_permission(
    sender_id: &str,
    target_id: &str,
    state: &Arc<AppState>,
) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    let is_host = { state.room_config.lock().unwrap().host_id == Some(sender_id.to_string()) };

    if is_host {
        state
            .unmute_permissions
            .lock()
            .unwrap()
            .insert(target_id.to_string());
        messages.push(ServerMessage::PermissionGranted {
            target_id: target_id.to_string(),
            media_type: "audio".to_string(),
        });
    }
    messages
}

pub fn grant_camera_permission(
    sender_id: &str,
    target_id: &str,
    state: &Arc<AppState>,
) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    let is_host = { state.room_config.lock().unwrap().host_id == Some(sender_id.to_string()) };

    if is_host {
        state
            .camera_permissions
            .lock()
            .unwrap()
            .insert(target_id.to_string());
        messages.push(ServerMessage::PermissionGranted {
            target_id: target_id.to_string(),
            media_type: "video".to_string(),
        });
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{Participant, PresenceStatus, RoomConfig};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::sync::broadcast;

    fn create_mock_state() -> Arc<AppState> {
        let (tx, _) = broadcast::channel(10);
        Arc::new(AppState {
            tx,
            participants: Arc::new(Mutex::new(HashMap::new())),
            knocking_participants: Arc::new(Mutex::new(HashMap::new())),
            room_config: Arc::new(Mutex::new(RoomConfig::default())),
            polls: Arc::new(Mutex::new(HashMap::new())),
            whiteboard: Arc::new(Mutex::new(Vec::new())),
            chat_history: Arc::new(Mutex::new(Vec::new())),
            breakout_rooms: Arc::new(Mutex::new(HashMap::new())),
            participant_locations: Arc::new(Mutex::new(HashMap::new())),
            shared_video_url: Arc::new(Mutex::new(None)),
            speaking_start_times: Arc::new(Mutex::new(HashMap::new())),
            feedback: Arc::new(Mutex::new(Vec::new())),
            remote_control_sessions: Arc::new(Mutex::new(HashMap::new())),
            pending_remote_control_requests: Arc::new(Mutex::new(std::collections::HashSet::new())),
            unmute_permissions: Arc::new(Mutex::new(std::collections::HashSet::new())),
            camera_permissions: Arc::new(Mutex::new(std::collections::HashSet::new())),
            feedback_timestamps: Arc::new(Mutex::new(HashMap::new())),
            poll_seq: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            admin_token: None,
        })
    }

    #[test]
    fn test_mute_camera_all_host() {
        let state = create_mock_state();
        let host_id = "host".to_string();
        let user_id = "user".to_string();

        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(host_id.clone());
        }

        {
            let mut locs = state.participant_locations.lock().unwrap();
            locs.insert(host_id.clone(), None);
            locs.insert(user_id.clone(), None);
        }

        {
            let mut p_map = state.participants.lock().unwrap();
            p_map.insert(
                host_id.clone(),
                Participant {
                    id: host_id.clone(),
                    name: "Host".to_string(),
                    is_hand_raised: false,
                    is_sharing_screen: false,
                    is_muted: false,
                    is_camera_muted: false,
                    speaking_time: 0,
                    presence: PresenceStatus::Connected,
                    is_visitor: false,
                    e2ee_enabled: false,
                    hand_raised_at: None,
                    avatar_url: None,
                },
            );
            p_map.insert(
                user_id.clone(),
                Participant {
                    id: user_id.clone(),
                    name: "User".to_string(),
                    is_hand_raised: false,
                    is_sharing_screen: false,
                    is_muted: false,
                    is_camera_muted: false,
                    speaking_time: 0,
                    presence: PresenceStatus::Connected,
                    is_visitor: false,
                    e2ee_enabled: false,
                    hand_raised_at: None,
                    avatar_url: None,
                },
            );
        }

        let msgs = mute_camera_all(&host_id, &state);
        assert!(
            msgs.iter()
                .any(|m| matches!(m, ServerMessage::CameraMutedByHost(id) if id == &user_id))
        );
    }

    #[test]
    fn test_mute_all_host() {
        let state = create_mock_state();
        let host_id = "host".to_string();
        let user_id = "user".to_string();

        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(host_id.clone());
        }

        {
            let mut p_map = state.participants.lock().unwrap();
            p_map.insert(
                host_id.clone(),
                Participant {
                    id: host_id.clone(),
                    name: "Host".to_string(),
                    is_hand_raised: false,
                    is_sharing_screen: false,
                    is_muted: false,
                    is_camera_muted: false,
                    speaking_time: 0,
                    presence: PresenceStatus::Connected,
                    is_visitor: false,
                    e2ee_enabled: false,
                    hand_raised_at: None,
                    avatar_url: None,
                },
            );
            p_map.insert(
                user_id.clone(),
                Participant {
                    id: user_id.clone(),
                    name: "User".to_string(),
                    is_hand_raised: false,
                    is_sharing_screen: false,
                    is_muted: false,
                    is_camera_muted: false,
                    speaking_time: 0,
                    presence: PresenceStatus::Connected,
                    is_visitor: false,
                    e2ee_enabled: false,
                    hand_raised_at: None,
                    avatar_url: None,
                },
            );
        }

        // Both participants must be in the same room (main room = None)
        {
            let mut locs = state.participant_locations.lock().unwrap();
            locs.insert(host_id.clone(), None);
            locs.insert(user_id.clone(), None);
        }

        let msgs = mute_all(&host_id, &state);
        assert!(!msgs.is_empty());
        assert!(
            msgs.iter()
                .any(|m| matches!(m, ServerMessage::MutedByHost(id) if id == &user_id))
        );

        let p_map = state.participants.lock().unwrap();
        assert!(p_map.get(&user_id).unwrap().is_muted);
        assert!(!p_map.get(&host_id).unwrap().is_muted);
    }

    #[test]
    fn test_mute_all_non_host() {
        let state = create_mock_state();
        let host_id = "host".to_string();
        let user_id = "user".to_string();

        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(host_id.clone());
        }

        let msgs = mute_all(&user_id, &state);
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_mute_all_scoped_to_room() {
        let state = create_mock_state();
        let host_id = "host".to_string();
        let user_in_main = "user_main".to_string();
        let user_in_breakout = "user_breakout".to_string();

        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(host_id.clone());
        }

        {
            let mut p_map = state.participants.lock().unwrap();
            for (id, name) in [
                (host_id.clone(), "Host"),
                (user_in_main.clone(), "MainUser"),
                (user_in_breakout.clone(), "BreakoutUser"),
            ] {
                p_map.insert(
                    id.clone(),
                    Participant {
                        id,
                        name: name.to_string(),
                        is_hand_raised: false,
                        is_sharing_screen: false,
                        is_muted: false,
                        is_camera_muted: false,
                        speaking_time: 0,
                        presence: PresenceStatus::Connected,
                        is_visitor: false,
                        e2ee_enabled: false,
                        hand_raised_at: None,
                        avatar_url: None,
                    },
                );
            }
        }

        // Host and user_in_main are in main room (None),
        // user_in_breakout is in a breakout room.
        {
            let mut locs = state.participant_locations.lock().unwrap();
            locs.insert(host_id.clone(), None);
            locs.insert(user_in_main.clone(), None);
            locs.insert(user_in_breakout.clone(), Some("room-1".to_string()));
        }

        let msgs = mute_all(&host_id, &state);

        // Only user_in_main should be muted (same room as host)
        assert!(
            msgs.iter()
                .any(|m| matches!(m, ServerMessage::MutedByHost(id) if id == &user_in_main))
        );
        assert!(
            !msgs
                .iter()
                .any(|m| matches!(m, ServerMessage::MutedByHost(id) if id == &user_in_breakout))
        );

        let p_map = state.participants.lock().unwrap();
        assert!(p_map.get(&user_in_main).unwrap().is_muted);
        assert!(!p_map.get(&user_in_breakout).unwrap().is_muted);
        assert!(!p_map.get(&host_id).unwrap().is_muted);
    }

    #[test]
    fn test_stop_screen_share_all_host() {
        let state = create_mock_state();
        let host_id = "host".to_string();
        let user_id = "user".to_string();

        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(host_id.clone());
        }

        {
            let mut locs = state.participant_locations.lock().unwrap();
            locs.insert(host_id.clone(), None);
            locs.insert(user_id.clone(), None);
        }

        {
            let mut p_map = state.participants.lock().unwrap();
            p_map.insert(
                host_id.clone(),
                Participant {
                    id: host_id.clone(),
                    name: "Host".to_string(),
                    is_hand_raised: false,
                    is_sharing_screen: false,
                    is_muted: false,
                    is_camera_muted: false,
                    speaking_time: 0,
                    presence: PresenceStatus::Connected,
                    is_visitor: false,
                    e2ee_enabled: false,
                    hand_raised_at: None,
                    avatar_url: None,
                },
            );
            p_map.insert(
                user_id.clone(),
                Participant {
                    id: user_id.clone(),
                    name: "User".to_string(),
                    is_hand_raised: false,
                    is_sharing_screen: true,
                    is_muted: false,
                    is_camera_muted: false,
                    speaking_time: 0,
                    presence: PresenceStatus::Connected,
                    is_visitor: false,
                    e2ee_enabled: false,
                    hand_raised_at: None,
                    avatar_url: None,
                },
            );
        }

        let msgs = stop_screen_share_all(&host_id, &state);
        assert!(
            msgs.iter()
                .any(|m| matches!(m, ServerMessage::ScreenShareStoppedByHost))
        );

        let p_map = state.participants.lock().unwrap();
        assert!(!p_map.get(&user_id).unwrap().is_sharing_screen);
    }

    #[test]
    fn test_unmute_permission_flow() {
        let state = create_mock_state();
        let user_id = "user1".to_string();
        let host_id = "host".to_string();

        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(host_id.clone());
            config.audio_moderation_enabled = true;
        }

        // Request permission
        let msgs = handle_unmute_permission_request(&user_id, &state);
        assert_eq!(msgs.len(), 1);
        assert!(
            matches!(msgs[0], ServerMessage::UnmutePermissionRequested { ref user_id } if user_id == "user1")
        );

        // Grant permission
        let msgs = grant_unmute_permission(&host_id, &user_id, &state);
        assert_eq!(msgs.len(), 1);
        assert!(
            matches!(msgs[0], ServerMessage::PermissionGranted { ref media_type, ref target_id } if media_type == "audio" && target_id == "user1")
        );

        let permissions = state.unmute_permissions.lock().unwrap();
        assert!(permissions.contains(&user_id));
    }
}
