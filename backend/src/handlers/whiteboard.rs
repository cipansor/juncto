use crate::AppState;
use shared::{DrawAction, ServerMessage};
use std::sync::Arc;

/// Result of a draw action: the message to broadcast, plus how many of the
/// oldest strokes were evicted. The caller broadcasts that count (not a full
/// snapshot) so clients can trim their history and repaint at a bounded
/// cadence, rather than receiving the whole history on every draw past the cap.
#[derive(Debug)]
pub struct DrawOutcome {
    pub message: ServerMessage,
    pub evicted: u32,
}

pub fn process_draw_action(
    user_id: &str,
    mut action: DrawAction,
    state: &Arc<AppState>,
) -> DrawOutcome {
    action.sender_id = user_id.to_string();
    let mut evicted: u32 = 0;
    {
        let mut wb = state.whiteboard.lock().unwrap();
        wb.push(action.clone());
        let excess = wb.len().saturating_sub(shared::MAX_WHITEBOARD_STROKES);
        if excess > 0 {
            wb.drain(0..excess);
            evicted = excess as u32;
        }
    }
    DrawOutcome {
        message: ServerMessage::Draw(action),
        evicted,
    }
}

/// Messages a draw contributes to the broadcast channel: the stroke itself, plus
/// an eviction notice when the cap was exceeded. Deliberately never a full
/// snapshot — sending one per draw once the cap is reached is the quadratic
/// blow-up this protocol avoids.
pub fn draw_broadcast(
    user_id: &str,
    action: DrawAction,
    state: &Arc<AppState>,
) -> Vec<ServerMessage> {
    let outcome = process_draw_action(user_id, action, state);
    let mut messages = vec![outcome.message];
    if outcome.evicted > 0 {
        messages.push(ServerMessage::WhiteboardEvicted {
            count: outcome.evicted,
        });
    }
    messages
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
            poll_seq: Arc::new(std::sync::atomic::AtomicU64::new(1)),
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
        assert_eq!(outcome.evicted, 0);

        let wb = state.whiteboard.lock().unwrap();
        assert_eq!(wb.len(), 1);
        assert_eq!(wb[0].sender_id, "user1");
    }

    #[test]
    fn test_whiteboard_history_is_bounded() {
        let state = create_mock_state();
        let mut total_evicted: u32 = 0;
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
            total_evicted += process_draw_action("user1", action, &state).evicted;
        }

        // Ten draws pushed past the cap, so exactly ten strokes must have been
        // reported as evicted (the count clients trim from the front).
        assert_eq!(total_evicted, 10);

        let wb = state.whiteboard.lock().unwrap();
        assert_eq!(wb.len(), shared::MAX_WHITEBOARD_STROKES);
        // Oldest strokes evicted, newest kept.
        assert_eq!(
            wb.last().unwrap().start_x,
            (shared::MAX_WHITEBOARD_STROKES + 9) as f64
        );
        assert_eq!(wb.first().unwrap().start_x, 10.0);
    }

    #[test]
    fn test_draw_never_broadcasts_full_snapshot_at_cap() {
        let state = create_mock_state();
        // Fill to the cap, then verify that the draws past it emit only the new
        // stroke plus a small eviction notice — never a 10k-stroke snapshot.
        for i in 0..shared::MAX_WHITEBOARD_STROKES {
            let action = DrawAction {
                color: "#000".to_string(),
                width: 1.0,
                start_x: i as f64,
                start_y: 0.0,
                end_x: i as f64,
                end_y: 1.0,
                sender_id: String::new(),
            };
            process_draw_action("user1", action, &state);
        }

        let action = DrawAction {
            color: "#000".to_string(),
            width: 1.0,
            start_x: 1.0,
            start_y: 1.0,
            end_x: 2.0,
            end_y: 2.0,
            sender_id: String::new(),
        };
        let messages = draw_broadcast("user1", action, &state);

        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0], ServerMessage::Draw(_)));
        match &messages[1] {
            ServerMessage::WhiteboardEvicted { count } => assert_eq!(*count, 1),
            other => panic!("expected eviction notice, got {other:?}"),
        }
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, ServerMessage::WhiteboardHistory(_))),
            "a draw must never re-broadcast the whole history"
        );
    }
}
