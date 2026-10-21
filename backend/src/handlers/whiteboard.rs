use crate::AppState;
use shared::{DrawAction, ServerMessage};
use std::sync::Arc;

/// Apply a draw and broadcast it (plus any eviction notice) while still holding
/// the whiteboard lock.
///
/// Holding the lock across the broadcast is deliberate: it makes mutation and
/// delivery one atomic step, so two sockets drawing concurrently cannot
/// interleave a mutation and its broadcast. Without it, clients could retain
/// strokes in a different order than the server (A stores, B stores+broadcasts,
/// A broadcasts) and the count-only eviction notice cannot repair that.
///
/// The stroke is stamped with a monotonic `rev` from a process-wide counter
/// (still assigned under the lock, so it matches retained order). A resynced
/// client drops any stroke whose rev it has already seen, so strokes replayed
/// from the broadcast buffer after a `WhiteboardHistory` snapshot are not
/// duplicated.
///
/// Returns the number of oldest strokes evicted (0 unless the cap was exceeded).
pub fn apply_draw(user_id: &str, mut action: DrawAction, state: &Arc<AppState>) -> u32 {
    let mut wb = state.whiteboard.lock().unwrap();
    action.sender_id = user_id.to_string();
    action.rev = state
        .whiteboard_rev
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    wb.push(action.clone());
    let excess = wb.len().saturating_sub(shared::MAX_WHITEBOARD_STROKES);
    if excess > 0 {
        wb.drain(0..excess);
    }
    let evicted = excess as u32;

    // Never a full snapshot here: sending the whole history on every draw past
    // the cap is the quadratic blow-up this protocol exists to avoid. Clients
    // resynchronize via `WhiteboardHistory` on join and after a broadcast lag.
    let _ = state.tx.send(ServerMessage::Draw(action));
    if evicted > 0 {
        let _ = state
            .tx
            .send(ServerMessage::WhiteboardEvicted { count: evicted });
    }
    evicted
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
            whiteboard_rev: Arc::new(std::sync::atomic::AtomicU64::new(1)),
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
            rev: 0,
        };

        let evicted = apply_draw("user1", action, &state);
        assert_eq!(evicted, 0);

        let wb = state.whiteboard.lock().unwrap();
        assert_eq!(wb.len(), 1);
        assert_eq!(wb[0].sender_id, "user1");
        assert_eq!(wb[0].end_x, 10.0);
        // The stroke is stamped with a non-zero revision under the lock.
        assert!(wb[0].rev > 0);
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
                rev: 0,
            };
            total_evicted += apply_draw("user1", action, &state);
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
        // Revisions are strictly increasing in retained order, so a client can
        // dedupe replayed strokes and detect gaps.
        assert!(wb.windows(2).all(|w| w[0].rev < w[1].rev));
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
                rev: 0,
            };
            apply_draw("user1", action, &state);
        }

        // Subscribe only now: the channel capacity is far below the cap, so a
        // subscription taken before the fill would be permanently lagged.
        let mut rx = state.tx.subscribe();
        let action = DrawAction {
            color: "#000".to_string(),
            width: 1.0,
            start_x: 1.0,
            start_y: 1.0,
            end_x: 2.0,
            end_y: 2.0,
            sender_id: String::new(),
            rev: 0,
        };
        apply_draw("user1", action, &state);

        // The draw past the cap emits the stroke plus a 1-stroke eviction
        // notice, and nothing else — in particular no full history snapshot.
        let first = rx.try_recv().expect("draw broadcast");
        assert!(matches!(first, ServerMessage::Draw(_)));
        let second = rx.try_recv().expect("eviction broadcast");
        match second {
            ServerMessage::WhiteboardEvicted { count } => assert_eq!(count, 1),
            other => panic!("expected eviction notice, got {other:?}"),
        }
        assert!(
            !matches!(rx.try_recv(), Ok(ServerMessage::WhiteboardHistory(_))),
            "a draw must never re-broadcast the whole history"
        );
    }
}
