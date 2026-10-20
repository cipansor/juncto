use crate::AppState;
use shared::{DrawAction, ServerMessage};
use std::sync::Arc;

/// Maximum number of whiteboard strokes retained in the in-memory history.
/// Older strokes are evicted so a client cannot grow server memory without
/// bound by drawing indefinitely.
pub const MAX_WHITEBOARD_STROKES: usize = 10_000;

pub fn process_draw_action(
    user_id: &str,
    mut action: DrawAction,
    state: &Arc<AppState>,
) -> ServerMessage {
    action.sender_id = user_id.to_string();
    {
        let mut wb = state.whiteboard.lock().unwrap();
        wb.push(action.clone());
        let excess = wb.len().saturating_sub(MAX_WHITEBOARD_STROKES);
        if excess > 0 {
            wb.drain(0..excess);
        }
    }
    ServerMessage::Draw(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::RoomConfig;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio::sync::broadcast;

    // Helper to create mock state
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
            admin_token: None,
        })
    }

    #[test]
    fn test_process_draw() {
        let state = create_mock_state();
        let action = DrawAction {
            color: "#000".to_string(),
            width: 2.0,
            start_x: 0.0,
            start_y: 0.0,
            end_x: 10.0,
            end_y: 10.0,
            sender_id: "".to_string(), // Should be overwritten
        };

        let msg = process_draw_action("user1", action, &state);

        match msg {
            ServerMessage::Draw(a) => {
                assert_eq!(a.sender_id, "user1");
                assert_eq!(a.end_x, 10.0);
            }
            _ => panic!("Wrong message type"),
        }

        let wb = state.whiteboard.lock().unwrap();
        assert_eq!(wb.len(), 1);
        assert_eq!(wb[0].sender_id, "user1");
    }

    #[test]
    fn test_whiteboard_history_is_bounded() {
        let state = create_mock_state();
        for i in 0..(MAX_WHITEBOARD_STROKES + 10) {
            let action = DrawAction {
                color: "#000".to_string(),
                width: 2.0,
                start_x: i as f64,
                start_y: 0.0,
                end_x: i as f64,
                end_y: 10.0,
                sender_id: "".to_string(),
            };
            process_draw_action("user1", action, &state);
        }

        let wb = state.whiteboard.lock().unwrap();
        assert_eq!(wb.len(), MAX_WHITEBOARD_STROKES);
        // Oldest strokes evicted, newest kept.
        assert_eq!(
            wb.last().unwrap().start_x,
            (MAX_WHITEBOARD_STROKES + 9) as f64
        );
        assert_eq!(wb.first().unwrap().start_x, 10.0);
    }
}
