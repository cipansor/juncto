use crate::AppState;
use shared::{DrawAction, ServerMessage};
use std::sync::Arc;

/// Result of a draw action: the message to broadcast, plus whether strokes were
/// evicted. When they were, the caller must also broadcast an authoritative
/// `WhiteboardHistory` snapshot so every client drops the evicted strokes.
#[derive(Debug)]
pub struct DrawOutcome {
    pub message: ServerMessage,
    pub evicted: bool,
}

pub fn process_draw_action(
    user_id: &str,
    mut action: DrawAction,
    state: &Arc<AppState>,
) -> DrawOutcome {
    action.sender_id = user_id.to_string();
    let mut evicted = false;
    {
        let mut wb = state.whiteboard.lock().unwrap();
        wb.push(action.clone());
        let excess = wb.len().saturating_sub(shared::MAX_WHITEBOARD_STROKES);
        if excess > 0 {
            wb.drain(0..excess);
            evicted = true;
        }
    }
    DrawOutcome {
        message: ServerMessage::Draw(action),
        evicted,
    }
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

        let outcome = process_draw_action("user1", action, &state);

        match outcome.message {
            ServerMessage::Draw(a) => {
                assert_eq!(a.sender_id, "user1");
                assert_eq!(a.end_x, 10.0);
            }
            _ => panic!("Wrong message type"),
        }
        assert!(!outcome.evicted);

        let wb = state.whiteboard.lock().unwrap();
        assert_eq!(wb.len(), 1);
        assert_eq!(wb[0].sender_id, "user1");
    }

    #[test]
    fn test_whiteboard_history_is_bounded() {
        let state = create_mock_state();
        let mut last_evicted = false;
        for i in 0..(shared::MAX_WHITEBOARD_STROKES + 10) {
            let action = DrawAction {
                color: "#000".to_string(),
                width: 2.0,
                start_x: i as f64,
                start_y: 0.0,
                end_x: i as f64,
                end_y: 10.0,
                sender_id: "".to_string(),
            };
            last_evicted = process_draw_action("user1", action, &state).evicted;
        }

        // The final draw pushed past the cap, so an eviction (and thus a
        // snapshot broadcast) must have been signalled.
        assert!(last_evicted);

        let wb = state.whiteboard.lock().unwrap();
        assert_eq!(wb.len(), shared::MAX_WHITEBOARD_STROKES);
        // Oldest strokes evicted, newest kept.
        assert_eq!(
            wb.last().unwrap().start_x,
            (shared::MAX_WHITEBOARD_STROKES + 9) as f64
        );
        assert_eq!(wb.first().unwrap().start_x, 10.0);
    }
}
