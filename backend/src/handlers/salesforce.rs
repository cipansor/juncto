use crate::AppState;
use shared::{SalesforceConfig, ServerMessage};

pub fn handle_link_salesforce(
    _uid: &str,
    config: SalesforceConfig,
    state: &AppState,
) -> Vec<ServerMessage> {
    let mut responses = Vec::new();

    // Persist to RoomConfig
    {
        let mut room_config = state.room_config.lock().unwrap();
        room_config.salesforce = config.clone();
    }

    // Broadcast update to everyone
    responses.push(ServerMessage::SalesforceUpdated(config.clone()));

    // Also send a general RoomUpdated for consistency
    let full_config = {
        let room_config = state.room_config.lock().unwrap();
        room_config.clone()
    };
    responses.push(ServerMessage::RoomUpdated(full_config));

    responses
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use shared::RoomConfig;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::sync::broadcast;

    fn create_mock_state() -> AppState {
        let (tx, _) = broadcast::channel(10);
        AppState {
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
        }
    }

    #[test]
    fn test_handle_link_salesforce() {
        let state = create_mock_state();
        let config = SalesforceConfig {
            is_linked: true,
            object_id: Some("001abc".to_string()),
            object_type: Some("Account".to_string()),
        };

        let responses = handle_link_salesforce("user1", config.clone(), &state);

        assert_eq!(responses.len(), 2);
        assert!(matches!(responses[0], ServerMessage::SalesforceUpdated(_)));
        assert!(matches!(responses[1], ServerMessage::RoomUpdated(_)));

        let saved_config = state.room_config.lock().unwrap().salesforce.clone();
        assert_eq!(saved_config, config);
    }
}
