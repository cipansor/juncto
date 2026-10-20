mod api;
mod handlers;

use axum::{
    Router,
    routing::{get, post},
};
use shared::{BreakoutRoom, DrawAction, Participant, Poll, RoomConfig, ServerMessage};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, oneshot};
use tower_http::services::{ServeDir, ServeFile};

type KnockingMap = HashMap<String, (Participant, Option<oneshot::Sender<bool>>)>;

// AppState to hold the broadcast channel and participants list
#[derive(Clone)]
pub struct AppState {
    pub tx: broadcast::Sender<ServerMessage>,
    pub participants: Arc<Mutex<HashMap<String, Participant>>>,
    pub knocking_participants: Arc<Mutex<KnockingMap>>,
    pub room_config: Arc<Mutex<RoomConfig>>,
    pub polls: Arc<Mutex<HashMap<String, Poll>>>,
    pub whiteboard: Arc<Mutex<Vec<DrawAction>>>,
    pub chat_history: Arc<Mutex<Vec<shared::ChatMessage>>>,
    pub breakout_rooms: Arc<Mutex<HashMap<String, BreakoutRoom>>>,
    // Track participants' current room: participant_id -> room_id (None = Main)
    pub participant_locations: Arc<Mutex<HashMap<String, Option<String>>>>,
    pub shared_video_url: Arc<Mutex<Option<String>>>,
    pub speaking_start_times: Arc<Mutex<HashMap<String, u64>>>,
    pub feedback: Arc<Mutex<Vec<shared::Feedback>>>,
    /// Active remote-control sessions: maps controller (requester) id ->
    /// controlled (target) id. Used by the handler to authorize subsequent
    /// `RemoteControlAction` and `StopRemoteControl` messages so a malicious
    /// client cannot inject actions without having been granted access.
    pub remote_control_sessions: Arc<Mutex<HashMap<String, String>>>,
    /// Pending remote-control requests: set of (requester_id, target_id)
    /// pairs that have been broadcast as `RemoteControlRequest` but not
    /// yet granted, denied, or expired. `GrantRemoteControl` requires a
    /// matching pending entry so a malicious client cannot fabricate a
    /// grant that activates the overlay on an unconsenting peer.
    pub pending_remote_control_requests: Arc<Mutex<HashSet<(String, String)>>>,
    pub unmute_permissions: Arc<Mutex<HashSet<String>>>,
    pub camera_permissions: Arc<Mutex<HashSet<String>>>,
    /// Rate limiting for feedback: tracks timestamps of submissions per user/IP
    pub feedback_timestamps: Arc<Mutex<HashMap<String, Vec<std::time::Instant>>>>,
    /// Monotonic sequence source for poll creation order. A process-wide counter
    /// (rather than the max of the retained polls) so `created_seq` stays unique
    /// even across replacements and evictions.
    pub poll_seq: Arc<std::sync::atomic::AtomicU64>,
    /// Shared secret required to reset the global room via `POST /api/rooms`.
    /// Read from `ROOM_ADMIN_TOKEN`; when unset the endpoint is disabled, so a
    /// self-hosted deployment is not trivially disrupted by any anonymous
    /// network client.
    pub admin_token: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Initialize broadcast channel (capacity 100)
    let (tx, _rx) = broadcast::channel(100);
    // Initialize participants list
    let participants = Arc::new(Mutex::new(HashMap::new()));
    // Initialize knocking participants list
    let knocking_participants = Arc::new(Mutex::new(HashMap::new()));
    // Initialize room config
    let room_config = Arc::new(Mutex::new(RoomConfig::default()));
    // Initialize polls
    let polls = Arc::new(Mutex::new(HashMap::new()));
    // Initialize whiteboard
    let whiteboard = Arc::new(Mutex::new(Vec::new()));
    // Initialize chat history
    let chat_history = Arc::new(Mutex::new(Vec::new()));
    // Initialize breakout rooms
    let breakout_rooms = Arc::new(Mutex::new(HashMap::new()));
    let participant_locations = Arc::new(Mutex::new(HashMap::new()));
    let shared_video_url = Arc::new(Mutex::new(None));
    let speaking_start_times = Arc::new(Mutex::new(HashMap::new()));
    let feedback = Arc::new(Mutex::new(Vec::new()));
    let remote_control_sessions = Arc::new(Mutex::new(HashMap::new()));
    let pending_remote_control_requests = Arc::new(Mutex::new(HashSet::new()));
    let unmute_permissions = Arc::new(Mutex::new(HashSet::new()));
    let camera_permissions = Arc::new(Mutex::new(HashSet::new()));
    let feedback_timestamps = Arc::new(Mutex::new(HashMap::new()));
    let poll_seq = Arc::new(std::sync::atomic::AtomicU64::new(1));
    // Optional shared secret for the room-reset endpoint. Absent (the default)
    // means the endpoint stays disabled rather than open.
    let admin_token = std::env::var("ROOM_ADMIN_TOKEN")
        .ok()
        .filter(|token| !token.is_empty());

    let app_state = Arc::new(AppState {
        tx,
        participants,
        knocking_participants,
        room_config,
        polls,
        whiteboard,
        chat_history,
        breakout_rooms,
        participant_locations,
        shared_video_url,
        speaking_start_times,
        feedback,
        remote_control_sessions,
        pending_remote_control_requests,
        unmute_permissions,
        camera_permissions,
        feedback_timestamps,
        poll_seq,
        admin_token,
    });

    // Resolve served paths against the crate manifest dir so the binary works
    // regardless of the process working directory (e.g. `cargo run` from the
    // workspace root or from tests/e2e).
    let frontend_pkg = format!("{}/../frontend/pkg", env!("CARGO_MANIFEST_DIR"));
    let static_dir = format!("{}/static", env!("CARGO_MANIFEST_DIR"));

    // Define the router
    let serve_dir = ServeDir::new(&frontend_pkg)
        .not_found_service(ServeFile::new(format!("{}/index.html", frontend_pkg)));
    let serve_static = ServeDir::new(&static_dir);

    let app = Router::new()
        .nest_service("/static", serve_static)
        .route("/api/rooms", post(api::create_room))
        .route("/api/feedback", post(api::submit_feedback))
        .route("/health", get(api::health_check))
        .route("/ws/chat", get(api::chat_handler))
        .fallback_service(serve_dir)
        .with_state(app_state);

    // Run the server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
