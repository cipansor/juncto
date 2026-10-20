use crate::AppState;
use shared::{Participant, ServerMessage};
use std::sync::Arc;

pub fn create_breakout_room(
    user_id: &str,
    name: String,
    state: &Arc<AppState>,
) -> Result<ServerMessage, String> {
    let is_host = {
        let config = state.room_config.lock().unwrap();
        config.host_id.as_deref() == Some(user_id)
    };

    if !is_host {
        return Err("Only host can create breakout rooms".to_string());
    }

    let id = uuid::Uuid::new_v4().to_string();
    let room = shared::BreakoutRoom {
        id: id.clone(),
        name,
    };

    {
        let mut rooms = state.breakout_rooms.lock().unwrap();
        rooms.insert(id, room);
    }

    let all_rooms: Vec<shared::BreakoutRoom> = {
        let rooms = state.breakout_rooms.lock().unwrap();
        rooms.values().cloned().collect()
    };

    Ok(ServerMessage::BreakoutRoomsList(all_rooms))
}

pub fn join_breakout_room(
    user_id: &str,
    room_id: Option<String>,
    state: &Arc<AppState>,
) -> Result<(Option<String>, Vec<ServerMessage>), String> {
    // Check if room exists if not None
    if let Some(rid) = &room_id {
        let rooms = state.breakout_rooms.lock().unwrap();
        if !rooms.contains_key(rid) {
            return Err("Breakout room not found".to_string());
        }
    }

    {
        let mut locations = state.participant_locations.lock().unwrap();
        locations.insert(user_id.to_string(), room_id.clone());
    }

    let mut messages = Vec::new();

    // If joining Main Room (None), return chat history to be sent to self
    if room_id.is_none() {
        let history = {
            let history = state.chat_history.lock().unwrap();
            history.clone()
        };
        if !history.is_empty() {
            messages.push(ServerMessage::ChatHistory(history));
        }
    }

    // Send updated participant list for the new room context
    // Filter participants by room
    let participants: Vec<Participant> = {
        let all_participants = state.participants.lock().unwrap();
        let locations = state.participant_locations.lock().unwrap();

        all_participants
            .values()
            .filter(|p| {
                let loc = locations.get(&p.id).cloned().flatten();
                loc == room_id
            })
            .cloned()
            .collect()
    };
    messages.push(ServerMessage::ParticipantList(participants));

    Ok((room_id, messages))
}

pub fn remove_breakout_room(
    user_id: &str,
    room_id: String,
    state: &Arc<AppState>,
) -> Result<Vec<ServerMessage>, String> {
    let is_host = {
        let config = state.room_config.lock().unwrap();
        config.host_id.as_deref() == Some(user_id)
    };

    if !is_host {
        return Err("Only host can remove breakout rooms".to_string());
    }

    let mut messages = Vec::new();

    // 1. Find all participants in this room and move them back to main
    let participant_ids: Vec<String> = {
        let locs = state.participant_locations.lock().unwrap();
        locs.iter()
            .filter(|(_, loc)| loc.as_deref() == Some(&room_id))
            .map(|(id, _)| id.clone())
            .collect()
    };

    for id in participant_ids {
        {
            let mut locs = state.participant_locations.lock().unwrap();
            locs.insert(id.clone(), None);
        }
        messages.push(ServerMessage::ForcedMoveToRoom {
            target_id: id,
            room_id: None,
        });
    }

    // 2. Remove the room
    {
        let mut rooms = state.breakout_rooms.lock().unwrap();
        rooms.remove(&room_id);
    }

    // 3. Broadcast updated list
    let all_rooms: Vec<shared::BreakoutRoom> = {
        let rooms = state.breakout_rooms.lock().unwrap();
        rooms.values().cloned().collect()
    };
    messages.push(ServerMessage::BreakoutRoomsList(all_rooms));

    Ok(messages)
}

pub fn rename_breakout_room(
    user_id: &str,
    room_id: String,
    new_name: String,
    state: &Arc<AppState>,
) -> Result<ServerMessage, String> {
    let is_host = {
        let config = state.room_config.lock().unwrap();
        config.host_id.as_deref() == Some(user_id)
    };

    if !is_host {
        return Err("Only host can rename breakout rooms".to_string());
    }

    {
        let mut rooms = state.breakout_rooms.lock().unwrap();
        if let Some(room) = rooms.get_mut(&room_id) {
            room.name = new_name;
        } else {
            return Err("Breakout room not found".to_string());
        }
    }

    let all_rooms: Vec<shared::BreakoutRoom> = {
        let rooms = state.breakout_rooms.lock().unwrap();
        rooms.values().cloned().collect()
    };

    Ok(ServerMessage::BreakoutRoomsList(all_rooms))
}

pub fn close_all_breakout_rooms(
    user_id: &str,
    state: &Arc<AppState>,
) -> Result<Vec<ServerMessage>, String> {
    let is_host = {
        let config = state.room_config.lock().unwrap();
        config.host_id.as_deref() == Some(user_id)
    };

    if !is_host {
        return Err("Only host can close breakout rooms".to_string());
    }

    // 1. Move all participants back to Main Room (None)
    let participant_ids: Vec<String> =
        { state.participants.lock().unwrap().keys().cloned().collect() };

    let mut messages = Vec::new();

    for id in &participant_ids {
        {
            let mut locations = state.participant_locations.lock().unwrap();
            locations.insert(id.clone(), None);
        }
        // Force everyone back to main room
        messages.push(ServerMessage::ForcedMoveToRoom {
            target_id: id.clone(),
            room_id: None,
        });
    }

    // 2. Clear all breakout rooms
    {
        let mut rooms = state.breakout_rooms.lock().unwrap();
        rooms.clear();
    }

    let messages = vec![ServerMessage::BreakoutRoomsList(Vec::new())];

    // Notify all participants they are moved back to main room
    // The WS loop will catch these and deliver them.
    // Actually, we should return messages that the WS handler will broadcast.
    // But broadcast::Sender is in AppState.

    Ok(messages)
}

pub fn move_participant_to_room(
    sender_id: &str,
    target_id: String,
    room_id: Option<String>,
    state: &Arc<AppState>,
) -> Result<Vec<ServerMessage>, String> {
    let is_host = {
        let config = state.room_config.lock().unwrap();
        config.host_id.as_deref() == Some(sender_id)
    };

    if !is_host {
        return Err("Only host can move participants".to_string());
    }

    // Validate target exists
    if !state.participants.lock().unwrap().contains_key(&target_id) {
        return Err("Target participant not found".to_string());
    }

    // Validate room exists if not None
    if let Some(rid) = &room_id
        && !state.breakout_rooms.lock().unwrap().contains_key(rid)
    {
        return Err("Breakout room not found".to_string());
    }

    {
        let mut locations = state.participant_locations.lock().unwrap();
        locations.insert(target_id.clone(), room_id.clone());
    }

    Ok(vec![ServerMessage::ForcedMoveToRoom { target_id, room_id }])
}

pub fn auto_assign_participants(
    sender_id: &str,
    state: &Arc<AppState>,
) -> Result<Vec<ServerMessage>, String> {
    let is_host = {
        let config = state.room_config.lock().unwrap();
        config.host_id.as_deref() == Some(sender_id)
    };

    if !is_host {
        return Err("Only host can auto-assign".to_string());
    }

    let rooms: Vec<String> = {
        state
            .breakout_rooms
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    };

    if rooms.is_empty() {
        return Err("No breakout rooms available".to_string());
    }

    let participants: Vec<String> = {
        state
            .participants
            .lock()
            .unwrap()
            .keys()
            .filter(|id| *id != sender_id) // Don't move host
            .cloned()
            .collect()
    };

    let mut messages = Vec::new();
    for (i, id) in participants.iter().enumerate() {
        let room_id = Some(rooms[i % rooms.len()].clone());
        {
            let mut locations = state.participant_locations.lock().unwrap();
            locations.insert(id.clone(), room_id.clone());
        }
        messages.push(ServerMessage::ForcedMoveToRoom {
            target_id: id.clone(),
            room_id,
        });
    }

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::RoomConfig;
    use std::collections::HashMap;
    use std::sync::Mutex;
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
    fn test_create_breakout_room_host() {
        let state = create_mock_state();
        let user_id = "host123";
        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(user_id.to_string());
        }

        let res = create_breakout_room(user_id, "Test Room".to_string(), &state);
        assert!(res.is_ok());
        if let Ok(ServerMessage::BreakoutRoomsList(rooms)) = res {
            assert_eq!(rooms.len(), 1);
            assert_eq!(rooms[0].name, "Test Room");
        } else {
            panic!("Wrong message type");
        }
    }

    #[test]
    fn test_create_breakout_room_not_host() {
        let state = create_mock_state();
        let user_id = "guest";
        let res = create_breakout_room(user_id, "Test Room".to_string(), &state);
        assert!(res.is_err());
    }

    #[test]
    fn test_join_breakout_room() {
        let state = create_mock_state();
        let user_id = "user1";
        let room_id = "room1".to_string();

        {
            let mut rooms = state.breakout_rooms.lock().unwrap();
            rooms.insert(
                room_id.clone(),
                shared::BreakoutRoom {
                    id: room_id.clone(),
                    name: "Room 1".to_string(),
                },
            );
        }

        let res = join_breakout_room(user_id, Some(room_id.clone()), &state);
        assert!(res.is_ok());

        let locs = state.participant_locations.lock().unwrap();
        assert_eq!(locs.get(user_id), Some(&Some(room_id)));
    }

    #[test]
    fn test_close_all_breakout_rooms() {
        let state = create_mock_state();
        let host_id = "host123";
        let user_id = "user1";
        let room_id = "room1".to_string();

        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(host_id.to_string());
            let mut rooms = state.breakout_rooms.lock().unwrap();
            rooms.insert(
                room_id.clone(),
                shared::BreakoutRoom {
                    id: room_id.clone(),
                    name: "Room 1".to_string(),
                },
            );
            let mut participants = state.participants.lock().unwrap();
            participants.insert(
                host_id.to_string(),
                shared::Participant {
                    id: host_id.to_string(),
                    name: "Host".to_string(),
                    is_hand_raised: false,
                    is_sharing_screen: false,
                    is_muted: false,
                    is_camera_muted: false,
                    speaking_time: 0,
                    presence: shared::PresenceStatus::Connected,
                    is_visitor: false,
                    e2ee_enabled: false,
                    hand_raised_at: None,
                    avatar_url: None,
                },
            );
            participants.insert(
                user_id.to_string(),
                shared::Participant {
                    id: user_id.to_string(),
                    name: "User".to_string(),
                    is_hand_raised: false,
                    is_sharing_screen: false,
                    is_muted: false,
                    is_camera_muted: false,
                    speaking_time: 0,
                    presence: shared::PresenceStatus::Connected,
                    is_visitor: false,
                    e2ee_enabled: false,
                    hand_raised_at: None,
                    avatar_url: None,
                },
            );
            let mut locs = state.participant_locations.lock().unwrap();
            locs.insert(host_id.to_string(), None);
            locs.insert(user_id.to_string(), Some(room_id.clone()));
        }

        let res = close_all_breakout_rooms(host_id, &state);
        assert!(res.is_ok());

        let locs = state.participant_locations.lock().unwrap();
        assert_eq!(locs.get(user_id), Some(&None));
        let rooms = state.breakout_rooms.lock().unwrap();
        assert!(rooms.is_empty());
    }

    #[test]
    fn test_auto_assign() {
        let state = create_mock_state();
        let host_id = "host123";
        let user1 = "u1";
        let user2 = "u2";

        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(host_id.to_string());
            let mut rooms = state.breakout_rooms.lock().unwrap();
            rooms.insert(
                "r1".to_string(),
                shared::BreakoutRoom {
                    id: "r1".to_string(),
                    name: "R1".to_string(),
                },
            );
            rooms.insert(
                "r2".to_string(),
                shared::BreakoutRoom {
                    id: "r2".to_string(),
                    name: "R2".to_string(),
                },
            );

            let mut participants = state.participants.lock().unwrap();
            for id in [host_id, user1, user2] {
                participants.insert(
                    id.to_string(),
                    shared::Participant {
                        id: id.to_string(),
                        name: id.to_string(),
                        is_hand_raised: false,
                        is_sharing_screen: false,
                        is_muted: false,
                        is_camera_muted: false,
                        speaking_time: 0,
                        presence: shared::PresenceStatus::Connected,
                        is_visitor: false,
                        e2ee_enabled: false,
                        hand_raised_at: None,
                        avatar_url: None,
                    },
                );
            }
        }

        let res = auto_assign_participants(host_id, &state);
        assert!(res.is_ok());

        let locs = state.participant_locations.lock().unwrap();
        assert!(locs.get(user1).unwrap().is_some());
        assert!(locs.get(user2).unwrap().is_some());
    }

    #[test]
    fn test_remove_breakout_room() {
        let state = create_mock_state();
        let host_id = "host123";
        let user_id = "user1";
        let room_id = "room1".to_string();

        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(host_id.to_string());
            let mut rooms = state.breakout_rooms.lock().unwrap();
            rooms.insert(
                room_id.clone(),
                shared::BreakoutRoom {
                    id: room_id.clone(),
                    name: "Room 1".to_string(),
                },
            );
            let mut participants = state.participants.lock().unwrap();
            participants.insert(
                user_id.to_string(),
                shared::Participant {
                    id: user_id.to_string(),
                    name: "User".to_string(),
                    is_hand_raised: false,
                    is_sharing_screen: false,
                    is_muted: false,
                    is_camera_muted: false,
                    speaking_time: 0,
                    presence: shared::PresenceStatus::Connected,
                    is_visitor: false,
                    e2ee_enabled: false,
                    hand_raised_at: None,
                    avatar_url: None,
                },
            );
            let mut locs = state.participant_locations.lock().unwrap();
            locs.insert(user_id.to_string(), Some(room_id.clone()));
        }

        let res = remove_breakout_room(host_id, room_id.clone(), &state);
        assert!(res.is_ok());

        let locs = state.participant_locations.lock().unwrap();
        assert_eq!(locs.get(user_id), Some(&None));
        let rooms = state.breakout_rooms.lock().unwrap();
        assert!(rooms.get(&room_id).is_none());
    }

    #[test]
    fn test_rename_breakout_room() {
        let state = create_mock_state();
        let host_id = "host123";
        let room_id = "room1".to_string();

        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(host_id.to_string());
            let mut rooms = state.breakout_rooms.lock().unwrap();
            rooms.insert(
                room_id.clone(),
                shared::BreakoutRoom {
                    id: room_id.clone(),
                    name: "Old Name".to_string(),
                },
            );
        }

        let res = rename_breakout_room(host_id, room_id.clone(), "New Name".to_string(), &state);
        assert!(res.is_ok());

        let rooms = state.breakout_rooms.lock().unwrap();
        assert_eq!(rooms.get(&room_id).unwrap().name, "New Name");
    }
}
