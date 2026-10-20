use super::*;
#[test]
fn test_chat_message_serialization() {
    let msg = ChatMessage {
        user_id: "user1".to_string(),
        content: "Hello Rust".to_string(),
        recipient_id: None,
        timestamp: 1627840000,
        attachment: None,
        room_id: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_chat_message_with_attachment_serialization() {
    let attachment = FileAttachment {
        filename: "test.txt".to_string(),
        mime_type: "text/plain".to_string(),
        size: 12,
        content_base64: "SGVsbG8gV29ybGQ=".to_string(),
    };
    let msg = ChatMessage {
        user_id: "user1".to_string(),
        content: "Here is a file".to_string(),
        recipient_id: None,
        timestamp: 1627840000,
        attachment: Some(attachment),
        room_id: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
    assert!(deserialized.attachment.is_some());
    assert_eq!(deserialized.attachment.unwrap().filename, "test.txt");
}

#[test]
fn test_server_message_serialization() {
    let p = Participant {
        id: "123".to_string(),
        name: "Alice".to_string(),
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
    };
    let msg = ServerMessage::ParticipantJoined(p.clone());
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ServerMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);

    let msg_update = ServerMessage::ParticipantUpdated(p.clone());
    let json_update = serde_json::to_string(&msg_update).unwrap();
    let deserialized_update: ServerMessage = serde_json::from_str(&json_update).unwrap();
    assert_eq!(msg_update, deserialized_update);

    let msg_reaction = ServerMessage::Reaction {
        sender_id: "123".to_string(),
        emoji: "👍".to_string(),
    };
    let json_reaction = serde_json::to_string(&msg_reaction).unwrap();
    let deserialized_reaction: ServerMessage = serde_json::from_str(&json_reaction).unwrap();
    assert_eq!(msg_reaction, deserialized_reaction);
}

#[test]
fn test_client_message_serialization() {
    let msg = ClientMessage::ToggleRoomLock(Some("secret".to_string()));
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);

    let msg_rec = ClientMessage::ToggleRecording;
    let json_rec = serde_json::to_string(&msg_rec).unwrap();
    let deserialized_rec: ClientMessage = serde_json::from_str(&json_rec).unwrap();
    assert_eq!(msg_rec, deserialized_rec);

    let msg_prof = ClientMessage::UpdateProfile("Bob".to_string());
    let json_prof = serde_json::to_string(&msg_prof).unwrap();
    let deserialized_prof: ClientMessage = serde_json::from_str(&json_prof).unwrap();
    assert_eq!(msg_prof, deserialized_prof);

    let msg_reaction = ClientMessage::Reaction("👍".to_string());
    let json_reaction = serde_json::to_string(&msg_reaction).unwrap();
    let deserialized_reaction: ClientMessage = serde_json::from_str(&json_reaction).unwrap();
    assert_eq!(msg_reaction, deserialized_reaction);

    let msg_hand = ClientMessage::ToggleRaiseHand;
    let json_hand = serde_json::to_string(&msg_hand).unwrap();
    let deserialized_hand: ClientMessage = serde_json::from_str(&json_hand).unwrap();
    assert_eq!(msg_hand, deserialized_hand);

    let msg_screen = ClientMessage::ToggleScreenShare;
    let json_screen = serde_json::to_string(&msg_screen).unwrap();
    let deserialized_screen: ClientMessage = serde_json::from_str(&json_screen).unwrap();
    assert_eq!(msg_screen, deserialized_screen);

    let msg_end = ClientMessage::EndMeeting;
    let json_end = serde_json::to_string(&msg_end).unwrap();
    let deserialized_end: ClientMessage = serde_json::from_str(&json_end).unwrap();
    assert_eq!(msg_end, deserialized_end);
}

#[test]
fn test_room_config_serialization() {
    let config = RoomConfig::default();
    assert!(!config.is_recording);
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: RoomConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, deserialized);
}

#[test]
fn test_poll_serialization() {
    let poll = Poll {
        id: "poll1".to_string(),
        question: "Color?".to_string(),
        options: vec![
            PollOption {
                id: 0,
                text: "Red".to_string(),
                votes: 0,
            },
            PollOption {
                id: 1,
                text: "Blue".to_string(),
                votes: 5,
            },
        ],
        voters: std::collections::HashSet::new(),
        is_closed: false,
    };
    let msg = ClientMessage::CreatePoll(poll.clone());
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_salesforce_config_serialization() {
    let config = SalesforceConfig {
        is_linked: true,
        object_id: Some("00Q...".to_string()),
        object_type: Some("Lead".to_string()),
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: SalesforceConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, deserialized);
}

#[test]
fn test_dropbox_config_serialization() {
    let config = DropboxConfig {
        is_connected: true,
        folder_path: Some("/juncto".to_string()),
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: DropboxConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, deserialized);
}

#[test]
fn test_salesforce_messages() {
    let config = SalesforceConfig {
        is_linked: true,
        object_id: Some("123".to_string()),
        object_type: Some("Contact".to_string()),
    };
    let msg = ClientMessage::LinkSalesforce(config.clone());
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);

    let msg_server = ServerMessage::SalesforceUpdated(config);
    let json_server = serde_json::to_string(&msg_server).unwrap();
    let deserialized_server: ServerMessage = serde_json::from_str(&json_server).unwrap();
    assert_eq!(msg_server, deserialized_server);
}

#[test]
fn test_dropbox_messages() {
    let msg = ClientMessage::SaveToDropbox("file.txt".to_string());
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);

    let msg_server = ServerMessage::DropboxSaveResult(true);
    let json_server = serde_json::to_string(&msg_server).unwrap();
    let deserialized_server: ServerMessage = serde_json::from_str(&json_server).unwrap();
    assert_eq!(msg_server, deserialized_server);
}

#[test]
fn test_etherpad_messages_serialization() {
    let msg = ClientMessage::SetEtherpadUrl(Some("https://pad.org/test".to_string()));
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);

    let msg_server = ServerMessage::EtherpadUrlUpdated {
        url: Some("https://pad.org/test".to_string()),
        room_id: Some("room1".to_string()),
    };
    let json_server = serde_json::to_string(&msg_server).unwrap();
    let deserialized_server: ServerMessage = serde_json::from_str(&json_server).unwrap();
    assert_eq!(msg_server, deserialized_server);
}

#[test]
fn test_giphy_messages_serialization() {
    let msg = ClientMessage::GiphyShare("https://giphy.com/gif1".to_string());
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);

    let msg_server = ServerMessage::GiphyShared {
        url: "https://giphy.com/gif1".to_string(),
        sender_id: "u1".to_string(),
        room_id: None,
    };
    let json_server = serde_json::to_string(&msg_server).unwrap();
    let deserialized_server: ServerMessage = serde_json::from_str(&json_server).unwrap();
    assert_eq!(msg_server, deserialized_server);
}

#[test]
fn test_mute_all_serialization() {
    let msg = ClientMessage::MuteAll;
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_transcription_serialization() {
    let msg = ServerMessage::Transcription {
        user_id: "u1".to_string(),
        text: "hello".to_string(),
        timestamp: 12345,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ServerMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_authenticate_message() {
    let msg = ClientMessage::Authenticate {
        username: "test_user".to_string(),
        password: Some("secret".to_string()),
    };
    let serialized = serde_json::to_string(&msg).unwrap();
    assert!(serialized.contains(r#""type":"Authenticate""#));
    assert!(serialized.contains(r#""username":"test_user""#));
    assert!(serialized.contains(r#""password":"secret""#));

    let server_msg = ServerMessage::AuthenticationResult(true);
    let server_serialized = serde_json::to_string(&server_msg).unwrap();
    assert!(server_serialized.contains(r#""type":"AuthenticationResult""#));
    assert!(server_serialized.contains(r#""payload":true"#));
}

#[test]
fn test_calendar_events_message() {
    let msg = ClientMessage::FetchCalendar;
    let serialized = serde_json::to_string(&msg).unwrap();
    assert!(serialized.contains(r#""type":"FetchCalendar""#));

    let server_msg = ServerMessage::CalendarEvents(vec!["Event 1".to_string()]);
    let server_serialized = serde_json::to_string(&server_msg).unwrap();
    assert!(server_serialized.contains(r#""type":"CalendarEvents""#));
    assert!(server_serialized.contains(r#""Event 1""#));
}

#[test]
fn test_analytics_event_message() {
    let msg = ClientMessage::AnalyticsEvent {
        name: "TestEvent".to_string(),
        properties: "{}".to_string(),
    };
    let serialized = serde_json::to_string(&msg).unwrap();
    assert!(serialized.contains(r#""type":"AnalyticsEvent""#));
    assert!(serialized.contains(r#""name":"TestEvent""#));
}

#[test]
fn test_shared_video_messages() {
    let msg = ClientMessage::StartShareVideo("https://youtu.be/test".to_string());
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);

    let msg = ClientMessage::StopShareVideo;
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);

    let msg = ServerMessage::VideoShared("https://youtu.be/test".to_string());
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ServerMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_speaking_message() {
    let msg = ClientMessage::Speaking(true);
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);

    let msg = ServerMessage::PeerSpeaking {
        user_id: "u1".to_string(),
        speaking: true,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ServerMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_presence_status_default() {
    let default_presence: PresenceStatus = Default::default();
    assert_eq!(default_presence, PresenceStatus::Connected);
}

#[test]
fn test_room_config_subtitles_default() {
    let config: RoomConfig = Default::default();
    assert!(!config.is_subtitles_enabled);
}

#[test]
fn test_is_giphy_cdn_url() {
    assert!(is_giphy_cdn_url(
        "https://media.giphy.com/media/abc/giphy.gif"
    ));
    assert!(is_giphy_cdn_url(
        "https://media0.giphy.com/media/abc/giphy.gif"
    ));
    assert!(is_giphy_cdn_url(
        "https://media4.giphy.com/media/abc/giphy.gif"
    ));
    assert!(is_giphy_cdn_url(
        "https://media5.giphy.com/media/abc/giphy.gif"
    ));
    assert!(is_giphy_cdn_url(
        "https://media9.giphy.com/media/abc/giphy.gif"
    ));
    assert!(is_giphy_cdn_url("https://i.giphy.com/abc.gif"));
    assert!(!is_giphy_cdn_url("https://evil.com/tracker.png"));
    assert!(!is_giphy_cdn_url("https://media.giphy.com@evil.com/"));
    assert!(!is_giphy_cdn_url(
        "https://mediaX.giphy.com/media/abc/giphy.gif"
    ));
    assert!(!is_giphy_cdn_url(
        "https://media99.giphy.com/media/abc/giphy.gif"
    ));
    assert!(!is_giphy_cdn_url("not-a-url"));
}

#[test]
fn test_set_subject_and_update_avatar_serialization() {
    let msg = ClientMessage::SetSubject(Some("Project Alpha".to_string()));
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);

    let msg = ClientMessage::UpdateAvatar(Some("http://avatar.com/1".to_string()));
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_draw_serialization() {
    let draw = DrawAction {
        color: "#000000".to_string(),
        start_x: 10.0,
        start_y: 20.0,
        end_x: 30.0,
        end_y: 40.0,
        width: 2.0,
        sender_id: "user1".to_string(),
    };
    let msg = ClientMessage::Draw(draw.clone());
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_is_valid_css_hex_color_accepts_hex_literals() {
    for valid in ["#fff", "#FFF", "#ffffff", "#0a1B2c", "#11223344"] {
        assert!(is_valid_css_hex_color(valid), "{valid} should be valid");
    }
}

#[test]
fn test_is_valid_css_hex_color_rejects_non_hex() {
    for invalid in [
        "",
        "fff",
        "#",
        "#12",
        "#12345",
        "#gggggg",
        "red",
        "var(--x)",
        "url(https://evil.example/x)",
        "#fff; background: url(https://evil.example/x)",
        "#fff\r\nX: y",
    ] {
        assert!(
            !is_valid_css_hex_color(invalid),
            "{invalid} should be rejected"
        );
    }
}

#[test]
fn test_is_valid_logo_url() {
    assert!(is_valid_logo_url("https://example.com/logo.png"));
    assert!(!is_valid_logo_url("http://example.com/logo.png"));
    assert!(!is_valid_logo_url("javascript:alert(1)"));
    assert!(!is_valid_logo_url("data:image/svg+xml,<svg/>"));
    assert!(!is_valid_logo_url("/relative/logo.png"));
    assert!(!is_valid_logo_url(""));
    let too_long = format!("https://example.com/{}", "a".repeat(2100));
    assert!(!is_valid_logo_url(&too_long));
}
