use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Feedback {
    pub stars: u8, // 1-5
    pub comment: String,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileAttachment {
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PowerStatus {
    pub battery_level: f64,
    pub is_charging: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SalesforceConfig {
    pub is_linked: bool,
    pub object_id: Option<String>,
    pub object_type: Option<String>, // e.g., "Lead", "Opportunity"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DropboxConfig {
    pub is_connected: bool,
    pub folder_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrandingConfig {
    pub primary_color: Option<String>,
    pub background_color: Option<String>,
    pub logo_url: Option<String>,
}

impl Default for BrandingConfig {
    fn default() -> Self {
        Self {
            primary_color: Some("#007bff".to_string()),
            background_color: Some("#ffffff".to_string()),
            logo_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomConfig {
    pub room_name: String,
    pub is_locked: bool,
    /// Access password for a locked room. Server-side only: never
    /// serialized into outbound messages. Incoming client messages that try
    /// to set it are ignored (see `Default` + `skip_deserializing`).
    #[serde(skip_serializing, skip_deserializing)]
    pub access_password: Option<String>,
    /// Public indicator that the lock is password-protected (safe to send).
    #[serde(default)]
    pub has_password: bool,
    pub is_recording: bool,
    pub is_lobby_enabled: bool,
    pub max_participants: u32,
    pub host_id: Option<String>,
    #[serde(default)]
    pub audio_moderation_enabled: bool,
    #[serde(default)]
    pub video_moderation_enabled: bool,
    #[serde(default)]
    pub e2ee_enabled: bool,
    #[serde(default)]
    pub is_subtitles_enabled: bool,
    #[serde(default)]
    pub etherpad_url: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub branding: BrandingConfig,
    #[serde(default)]
    pub salesforce: SalesforceConfig,
    #[serde(default)]
    pub dropbox: DropboxConfig,
}

impl Default for RoomConfig {
    fn default() -> Self {
        Self {
            room_name: "Default Room".to_string(),
            is_locked: false,
            access_password: None,
            has_password: false,
            is_recording: false,
            is_lobby_enabled: false,
            max_participants: 100,
            host_id: None,
            audio_moderation_enabled: false,
            video_moderation_enabled: false,
            e2ee_enabled: false,
            is_subtitles_enabled: false,
            etherpad_url: None,
            subject: None,
            branding: BrandingConfig::default(),
            salesforce: SalesforceConfig::default(),
            dropbox: DropboxConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserIdentity {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DrawAction {
    pub color: String,
    pub start_x: f64,
    pub start_y: f64,
    pub end_x: f64,
    pub end_y: f64,
    pub width: f64,
    #[serde(default)] // For backward compatibility if needed, though we are migrating fresh
    pub sender_id: String,
    /// Monotonic revision assigned by the server in whiteboard order. A client
    /// ignores any stroke whose `rev` is not greater than the newest it already
    /// holds, so strokes replayed after a resync snapshot (see the lag path in
    /// the broadcast task) are not duplicated. Client-sent draws leave this at
    /// 0; the server overwrites it.
    #[serde(default)]
    pub rev: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    /// Server-assigned unique identifier. Deduplication (a `Chat` replayed from
    /// the broadcast buffer after a `ChatHistory` resync) keys off this rather
    /// than `(user_id, timestamp, content)`, which collides for two identical
    /// messages sent in the same millisecond or for file-only messages with
    /// empty content. Empty for messages from older servers.
    #[serde(default)]
    pub id: String,
    pub user_id: String,
    pub content: String,
    pub recipient_id: Option<String>,
    pub timestamp: u64,
    #[serde(default)] // Default to None for backward compatibility during migration
    pub attachment: Option<FileAttachment>,
    #[serde(default)]
    pub room_id: Option<String>,
}

/// Whether two messages are the same logical message, for de-duplicating a
/// `Chat` replayed from the broadcast buffer against a `ChatHistory` snapshot.
/// Uses the server-assigned `id` when both carry one; falls back to the full
/// field tuple for legacy messages so two distinct same-millisecond messages,
/// or file-only messages with empty content, are not collapsed into one.
pub fn is_same_chat(a: &ChatMessage, b: &ChatMessage) -> bool {
    if !a.id.is_empty() && !b.id.is_empty() {
        return a.id == b.id;
    }
    a.user_id == b.user_id
        && a.timestamp == b.timestamp
        && a.content == b.content
        && a.recipient_id == b.recipient_id
        && a.room_id == b.room_id
        && a.attachment == b.attachment
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, Eq, Hash)]
pub enum PresenceStatus {
    #[default]
    Connected,
    Disconnected,
    Busy,
    Calling,
    Ringing,
    Rejected,
    Ignored,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Participant {
    pub id: String,
    pub name: String,
    pub is_hand_raised: bool,
    pub is_sharing_screen: bool,
    #[serde(default)]
    pub is_muted: bool,
    #[serde(default)]
    pub is_camera_muted: bool,
    #[serde(default)]
    pub speaking_time: u64, // Total milliseconds spoken
    #[serde(default)]
    pub presence: PresenceStatus,
    /// Whether this participant joined as a visitor (read-only mode).
    /// Currently always `false`; reserved for future visitor-role support.
    #[serde(default)]
    pub is_visitor: bool,
    /// Whether this participant has end-to-end encryption enabled locally.
    /// Updated via `ClientMessage::UpdateE2EE`; the frontend does not yet
    /// expose a UI toggle for this field.
    #[serde(default)]
    pub e2ee_enabled: bool,
    #[serde(default)]
    pub hand_raised_at: Option<u64>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PollOption {
    pub id: u32,
    pub text: String,
    pub votes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Poll {
    pub id: String,
    pub question: String,
    pub options: Vec<PollOption>,
    #[serde(default)]
    pub voters: HashSet<String>,
    #[serde(default)]
    pub is_closed: bool,
    /// Monotonic creation sequence used to evict the oldest poll
    /// deterministically once `MAX_POLLS` is reached. Assigned by the server;
    /// `HashMap` iteration order does not track insertion age.
    #[serde(default)]
    pub created_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceExpression {
    pub expression: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum RemoteControlAction {
    MouseMove { x: f64, y: f64 },
    MouseDown { button: u8 },
    MouseUp { button: u8 },
    KeyDown { key: String },
    KeyUp { key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum ClientMessage {
    CreatePoll(Poll),
    Vote {
        poll_id: String,
        option_id: u32,
    },
    Join {
        name: String,
        #[serde(default)]
        is_visitor: bool,
        #[serde(default)]
        avatar_url: Option<String>,
        /// Room access password, required when the room is locked with a
        /// password. Never logged or echoed back to other participants.
        #[serde(default)]
        password: Option<String>,
    },
    Chat {
        content: String,
        recipient_id: Option<String>,
        attachment: Option<FileAttachment>,
        #[serde(default)]
        room_id: Option<String>,
    },
    /// Lock/unlock the room. When locking with `Some(password)`, the password
    /// is stored on the room and required for subsequent joins. `None` locks
    /// without a password (joins are rejected outright).
    ToggleRoomLock(Option<String>),
    ToggleRecording,
    ToggleAudioModeration,
    ToggleVideoModeration,
    UpdateProfile(String), // New Name
    Reaction(String),      // Emoji
    ToggleRaiseHand,
    ToggleScreenShare,
    ToggleLobby,
    ToggleE2EE,
    SetEtherpadUrl(Option<String>),
    GiphyShare(String),
    GrantAccess(String),
    DenyAccess(String),
    KickParticipant(String), // Target ID
    MuteParticipant(String), // Target ID
    TransferHost(String),    // Target ID
    SetMuteStatus(bool),
    SetCameraMuteStatus(bool),
    EndMeeting,
    MuteCameraParticipant(String), // Target ID
    MuteCameraAll,
    SetPresence(PresenceStatus),
    CreateBreakoutRoom(String),       // Room Name
    JoinBreakoutRoom(Option<String>), // Room ID (None for Main)
    CloseAllBreakoutRooms,
    RemoveBreakoutRoom(String), // Room ID
    RenameBreakoutRoom {
        room_id: String,
        new_name: String,
    },
    MoveParticipantToRoom {
        target_id: String,
        room_id: Option<String>,
    },
    AutoAssignToBreakoutRooms,
    Draw(DrawAction),
    ToggleSubtitles,
    Typing(bool),
    StartShareVideo(String), // URL
    StopShareVideo,
    SetSubject(Option<String>),
    UpdateAvatar(Option<String>),
    Speaking(bool),
    Ping,
    MuteAll,
    StopScreenShareAll,
    SetBranding(BrandingConfig),
    UpdatePowerStatus(PowerStatus),
    RequestUnmute(String), // Target ID
    ToggleLocalRecording(bool),
    FollowMe(String),  // Layout name (e.g., "grid", "spotlight")
    ClosePoll(String), // Poll ID
    FaceExpression(FaceExpression),
    RequestRemoteControl(String), // Target ID
    GrantRemoteControl(String),   // Requester ID
    DenyRemoteControl(String),    // Requester ID
    StopRemoteControl(String),    // Peer ID
    RemoteControlAction {
        target_id: String,
        action: RemoteControlAction,
    },
    /// Update this participant's per-user E2EE status. The server handler
    /// exists (`ws.rs: UpdateE2EE`) but no frontend UI sends this message yet.
    /// Kept for protocol completeness; wire up when the E2EE settings panel
    /// is migrated.
    UpdateE2EE(bool),
    RequestUnmutePermission,
    RequestCameraPermission,
    GrantUnmutePermission(String),
    GrantCameraPermission(String),
    SetAudioOnly(bool),
    FlipLocalVideo(bool),
    PinParticipant(Option<String>),
    SetParticipantVolume {
        target_id: String,
        volume: f64,
    },
    MuteEveryoneElse(String), // Target ID
    E2EEKeyExchange(String),  // Key hash or public key
    BroadcastToLobby(String),
    PromoteVisitor(String),
    // WebRTC Signaling
    Offer {
        target_id: String,
        sdp: String,
    },
    Answer {
        target_id: String,
        sdp: String,
    },
    IceCandidate {
        target_id: String,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_m_line_index: Option<u16>,
    },
    Authenticate {
        username: String,
        password: Option<String>,
    },
    FetchCalendar,
    LinkSalesforce(SalesforceConfig),
    SaveToDropbox(String), // Filename or File ID
    AnalyticsEvent {
        name: String,
        properties: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BreakoutRoom {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum ServerMessage {
    Chat {
        message: ChatMessage,
        #[serde(default)]
        room_id: Option<String>,
    },
    PeerTyping {
        user_id: String,
        is_typing: bool,
        #[serde(default)]
        room_id: Option<String>,
    },
    Kicked {
        target_id: String,
        room_id: Option<String>,
    },
    MutedByHost(String), // Target ID (Broadcasted, filtered by client)
    BreakoutRoomsList(Vec<BreakoutRoom>),
    ParticipantJoined(Participant),
    ParticipantLeft {
        id: String,
        #[serde(default)]
        room_id: Option<String>,
    },
    ParticipantList(Vec<Participant>),
    KnockingParticipant(Participant),
    KnockingParticipantLeft(String), // ID
    RoomUpdated(RoomConfig),
    ParticipantUpdated(Participant),
    Reaction {
        sender_id: String,
        emoji: String,
    },
    PollCreated(Poll),
    PollUpdated(Poll),
    PollsList(Vec<Poll>),
    /// Poll IDs evicted from server state to stay under the retention cap.
    /// Connected clients must drop them so their poll list stays in sync.
    PollRemoved(Vec<String>),
    Draw(DrawAction),
    /// Number of oldest strokes the server dropped from its retained history
    /// to stay under `MAX_WHITEBOARD_STROKES`. Clients trim the same count from
    /// the front of their history and repaint at a bounded cadence, instead of
    /// receiving (and repainting) the full history on every draw past the cap.
    WhiteboardEvicted {
        count: u32,
    },
    /// Full authoritative snapshot of the retained strokes. Sent on join
    /// rather than on every eviction, so steady-state drawing stays incremental.
    WhiteboardHistory(Vec<DrawAction>),
    ChatHistory(Vec<ChatMessage>),
    Welcome {
        id: String,
    },
    Knocking,
    AccessDenied,
    EtherpadUrlUpdated {
        url: Option<String>,
        room_id: Option<String>,
    },
    GiphyShared {
        url: String,
        sender_id: String,
        room_id: Option<String>,
    },
    RoomEnded,
    VideoShared(String), // URL
    VideoStopped,
    PowerStatusUpdated {
        user_id: String,
        status: PowerStatus,
    },
    UnmuteRequested {
        requester_id: String,
        target_id: String,
    },
    ScreenShareStoppedByHost,
    RecordingStatusChanged {
        user_id: String,
        is_recording: bool,
    },
    PeerSpeaking {
        user_id: String,
        speaking: bool,
    },
    Pong {
        timestamp: u64,
    },
    Transcription {
        user_id: String,
        text: String,
        timestamp: u64,
    },
    FollowMe(String),   // Layout name
    PollClosed(String), // Poll ID
    FaceExpression {
        sender_id: String,
        expression: FaceExpression,
    },
    RemoteControlRequest {
        requester_id: String,
        target_id: String,
    },
    RemoteControlAllowed {
        requester_id: String,
        target_id: String, // The person who granted it
        allowed: bool,
    },
    RemoteControlStopped {
        sender_id: String,
        /// The peer on the other side of the session (controller if
        /// `sender_id` is the controlled, or controlled if `sender_id`
        /// is the controller). Used by the broadcast filter to deliver
        /// this message only to the two participants involved.
        peer_id: String,
    },
    RemoteControlAction {
        requester_id: String,
        target_id: String,
        action: RemoteControlAction,
    },
    LobbyAnnouncement(String),
    CameraMutedByHost(String), // Target ID
    VisitorPromoted(String),   // ID
    AudioOnlyChanged {
        user_id: String,
        enabled: bool,
    },
    ParticipantPinned {
        user_id: String,
        target_id: Option<String>,
    },
    ParticipantVolumeChanged {
        user_id: String,
        target_id: String,
        volume: f64,
    },
    UnmutePermissionRequested {
        user_id: String,
    },
    CameraPermissionRequested {
        user_id: String,
    },
    PermissionGranted {
        target_id: String,
        media_type: String,
    },
    ForcedMoveToRoom {
        target_id: String,
        room_id: Option<String>,
    },
    E2EEKeyExchange {
        from_id: String,
        key_hash: String,
    },
    // WebRTC Signaling
    Offer {
        source_id: String,
        target_id: String,
        sdp: String,
    },
    Answer {
        source_id: String,
        target_id: String,
        sdp: String,
    },
    IceCandidate {
        source_id: String,
        target_id: String,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_m_line_index: Option<u16>,
    },
    AuthenticationResult(bool),
    CalendarEvents(Vec<String>),
    SalesforceUpdated(SalesforceConfig),
    DropboxSaveResult(bool),
    Error(String),
}

/// Returns `true` if the given URL belongs to a known Giphy CDN domain.
/// Used on both the backend (chat validation, WS handler) and the frontend
/// (chat rendering) to enforce a consistent allowlist.
///
/// Recognised CDN hosts: `media.giphy.com`, `media0`–`media9.giphy.com`,
/// and `i.giphy.com`.  The check uses `starts_with` with a trailing `/`
/// to prevent authority-based bypasses (e.g. `https://media.giphy.com@evil.com`).
pub fn is_giphy_cdn_url(url: &str) -> bool {
    if let Some(rest) = url.strip_prefix("https://media") {
        // Matches "media.giphy.com/" or "media0.giphy.com/" … "media9.giphy.com/"
        if rest.starts_with(".giphy.com/") {
            return true;
        }
        if let Some(after_digit) = rest.strip_prefix(|c: char| c.is_ascii_digit()) {
            return after_digit.starts_with(".giphy.com/");
        }
        false
    } else {
        url.starts_with("https://i.giphy.com/")
    }
}

/// Maximum length (in Unicode scalar values) accepted for a participant
/// display name. Enforced by the backend on `Join` and `UpdateProfile`, and
/// mirrored by the client so users get immediate feedback.
pub const MAX_PARTICIPANT_NAME_LEN: usize = 64;

/// Maximum number of chat messages retained. Enforced by the server on the
/// authoritative history and mirrored by the client so a long-lived browser
/// tab cannot accumulate more messages than the server would replay.
pub const MAX_CHAT_HISTORY: usize = 500;

/// Maximum number of whiteboard strokes retained. Enforced by the server on
/// the authoritative history and mirrored by the client so a long-lived
/// browser tab cannot accumulate more strokes than the server would replay.
pub const MAX_WHITEBOARD_STROKES: usize = 10_000;

/// Maximum number of polls retained. Enforced by the server on the
/// authoritative map and mirrored by the client so a long-lived browser tab
/// cannot accumulate more polls than the server holds.
pub const MAX_POLLS: usize = 200;

/// Returns `true` if `value` is a CSS colour literal that is safe to assign to
/// a CSS custom property: `#rgb`, `#rrggbb` or `#rrggbbaa` hex.
///
/// Branding colours arrive from a client and are written straight into
/// `--primary-color` / `--background-color`, so anything that is not a plain
/// hex literal (a `url(...)`, a `;`-separated declaration, `var(...)`, …) must
/// be rejected rather than forwarded to every participant's stylesheet.
pub fn is_valid_css_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Returns `true` if `url` is an acceptable branding logo URL: an absolute
/// `https://` URL of at most 2048 characters.
///
/// Mirrors the avatar-URL rules so a host cannot make other participants'
/// browsers issue `http://` (mixed-content / tracking) or other-scheme
/// requests through `SetBranding`.
pub fn is_valid_logo_url(url: &str) -> bool {
    url.starts_with("https://") && url.len() <= 2048
}

#[cfg(test)]
mod tests;
