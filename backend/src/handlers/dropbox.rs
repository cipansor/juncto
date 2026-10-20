use crate::AppState;
use shared::ServerMessage;

pub fn handle_save_to_dropbox(
    _uid: &str,
    filename: String,
    _state: &AppState,
) -> Vec<ServerMessage> {
    let mut responses = Vec::new();

    // Mock success for now
    println!("INFO: Saving file {} to Dropbox (mock)", filename);
    responses.push(ServerMessage::DropboxSaveResult(true));

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
            whiteboard_rev: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            admin_token: None,
        }
    }

    #[test]
    fn test_handle_save_to_dropbox() {
        let state = create_mock_state();
        let responses = handle_save_to_dropbox("user1", "test_file.txt".to_string(), &state);

        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            ServerMessage::DropboxSaveResult(true)
        ));
    }
}
