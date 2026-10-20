use crate::AppState;
use shared::{ChatMessage, FileAttachment, ServerMessage};
use std::sync::Arc;

/// Maximum length (in Unicode scalar values) of a single chat message body.
pub const MAX_CHAT_CONTENT_LEN: usize = 4000;

/// Maximum size of a base64 file attachment carried by a chat message.
pub const MAX_ATTACHMENT_BYTES: usize = 3 * 1024 * 1024;

pub fn process_chat_message(
    user_id: &str,
    room_id: &Option<String>,
    content: String,
    recipient_id: Option<String>,
    attachment: Option<FileAttachment>,
    state: &Arc<AppState>,
) -> Result<ServerMessage, String> {
    if content.chars().count() > MAX_CHAT_CONTENT_LEN {
        return Err("Message too long".to_string());
    }

    if let Some(att) = &attachment {
        // Limit 3MB
        if att.content_base64.len() > MAX_ATTACHMENT_BYTES {
            return Err("File too large".to_string());
        }
    }

    // Server-side validation for GIF messages: only allow known Giphy CDN domains
    if let Some(url) = content.strip_prefix("GIF:")
        && !shared::is_giphy_cdn_url(url)
    {
        return Err("Invalid GIF URL: only Giphy CDN URLs are allowed".to_string());
    }

    let chat_msg = ChatMessage {
        user_id: user_id.to_string(),
        content,
        recipient_id: recipient_id.clone(),
        timestamp: chrono::Utc::now().timestamp_millis() as u64,
        attachment,
        room_id: room_id.clone(),
    };

    // Only store history for global chat (main room, public messages)
    if recipient_id.is_none() && room_id.is_none() {
        let mut history = state.chat_history.lock().unwrap();
        history.push(chat_msg.clone());
        // Ring-buffer semantics: drop the oldest entries once the cap is
        // reached so memory stays bounded across the room's lifetime.
        let excess = history.len().saturating_sub(shared::MAX_CHAT_HISTORY);
        if excess > 0 {
            history.drain(0..excess);
        }
    }

    Ok(ServerMessage::Chat {
        message: chat_msg,
        room_id: room_id.clone(),
    })
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
    fn test_process_chat_valid() {
        let state = create_mock_state();
        let res = process_chat_message("user1", &None, "hello".to_string(), None, None, &state);
        assert!(res.is_ok());
        if let Ok(ServerMessage::Chat { message, .. }) = res {
            assert_eq!(message.content, "hello");
            assert_eq!(message.user_id, "user1");
        } else {
            panic!("Wrong result");
        }

        // Check history
        let history = state.chat_history.lock().unwrap();
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_process_chat_gif_url_validation() {
        let state = create_mock_state();

        // Valid Giphy URL should succeed
        let res = process_chat_message(
            "user1",
            &None,
            "GIF:https://media.giphy.com/media/abc/giphy.gif".to_string(),
            None,
            None,
            &state,
        );
        assert!(res.is_ok());

        // Invalid GIF URL should be rejected
        let res = process_chat_message(
            "user1",
            &None,
            "GIF:https://evil.com/tracker.png".to_string(),
            None,
            None,
            &state,
        );
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err(),
            "Invalid GIF URL: only Giphy CDN URLs are allowed"
        );

        // Non-GIF messages should pass through unchanged
        let res = process_chat_message(
            "user1",
            &None,
            "GIF:not-a-url".to_string(),
            None,
            None,
            &state,
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_process_chat_too_large() {
        let state = create_mock_state();
        let big_data = "a".repeat(4 * 1024 * 1024); // 4MB
        let attachment = Some(FileAttachment {
            filename: "big.txt".to_string(),
            mime_type: "text/plain".to_string(),
            size: 4000000,
            content_base64: big_data,
        });

        let res =
            process_chat_message("user1", &None, "file".to_string(), None, attachment, &state);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "File too large");
    }

    #[test]
    fn test_process_chat_content_too_long() {
        let state = create_mock_state();
        let long = "a".repeat(MAX_CHAT_CONTENT_LEN + 1);
        let res = process_chat_message("user1", &None, long, None, None, &state);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Message too long");
        assert!(state.chat_history.lock().unwrap().is_empty());
    }

    #[test]
    fn test_chat_history_is_bounded() {
        let state = create_mock_state();
        for i in 0..(shared::MAX_CHAT_HISTORY + 25) {
            let res = process_chat_message("user1", &None, format!("msg {i}"), None, None, &state);
            assert!(res.is_ok());
        }

        let history = state.chat_history.lock().unwrap();
        assert_eq!(history.len(), shared::MAX_CHAT_HISTORY);
        // Oldest messages were evicted, newest retained.
        assert_eq!(
            history.last().unwrap().content,
            format!("msg {}", shared::MAX_CHAT_HISTORY + 24)
        );
        assert_eq!(history.first().unwrap().content, "msg 25");
    }
}
