use crate::AppState;
use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde_json::json;
use shared::RoomConfig;
use std::sync::Arc;

/// Constant-time comparison so the admin-token check does not leak the token
/// length or content through response timing.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= (a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0)) as usize;
    }
    diff == 0
}

/// Extracts the token from an `Authorization` header value. The scheme name is
/// case-insensitive per RFC 7235, so `bearer`, `Bearer` and `BEARER` are all
/// accepted; a missing or differently-schemed header yields `None`.
fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then_some(token)
}

pub async fn create_room(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RoomConfig>,
) -> impl IntoResponse {
    // Resetting the single global room wipes every participant, message and
    // stroke, so it must not be reachable anonymously. The endpoint is
    // disabled entirely unless `ROOM_ADMIN_TOKEN` is configured; when it is,
    // callers must present the matching bearer token.
    let authorized = match state.admin_token.as_deref() {
        Some(expected) => headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(bearer_token)
            .is_some_and(|provided| constant_time_eq(provided, expected)),
        None => false,
    };
    if !authorized {
        let err = json!({ "error": "Unauthorized" });
        return (StatusCode::UNAUTHORIZED, Json(err)).into_response();
    }

    // Branding is applied to every joining participant's stylesheet and logo
    // `<img>`, so enforce the same rules as the WebSocket `SetBranding` path.
    // Without this a token-holder could bypass the CSS/URL validation there.
    if let Some(ref color) = payload.branding.primary_color
        && !shared::is_valid_css_hex_color(color)
    {
        let err = json!({ "error": "Invalid primary color: expected a hex value like #RRGGBB" });
        return (StatusCode::BAD_REQUEST, Json(err)).into_response();
    }
    if let Some(ref color) = payload.branding.background_color
        && !shared::is_valid_css_hex_color(color)
    {
        let err = json!({ "error": "Invalid background color: expected a hex value like #RRGGBB" });
        return (StatusCode::BAD_REQUEST, Json(err)).into_response();
    }
    if let Some(ref logo) = payload.branding.logo_url
        && !shared::is_valid_logo_url(logo)
    {
        let err = json!({ "error": "Invalid logo URL: must start with https://" });
        return (StatusCode::BAD_REQUEST, Json(err)).into_response();
    }

    // Mirror the validation enforced by the WebSocket `SetSubject` handler
    // so the REST endpoint cannot be used to bypass the 256-character limit.
    // Use `chars().count()` (not byte length) so multi-byte UTF-8 content
    // (CJK, emoji) is treated consistently with what users see.
    if let Some(ref s) = payload.subject
        && s.chars().count() > 256
    {
        let err = json!({ "error": "Invalid subject: too long" });
        return (StatusCode::BAD_REQUEST, Json(err)).into_response();
    }

    if payload.room_name.chars().count() > 128 {
        let err = json!({ "error": "Invalid room name: too long" });
        return (StatusCode::BAD_REQUEST, Json(err)).into_response();
    }

    // A zero (or absurd) capacity would otherwise lock every participant out
    // of the room, which is indistinguishable from a denial of service.
    if payload.max_participants == 0 || payload.max_participants > 1000 {
        let err = json!({ "error": "Invalid max_participants: must be between 1 and 1000" });
        return (StatusCode::BAD_REQUEST, Json(err)).into_response();
    }

    // Normalize empty subject to `None` for defensive consistency with the
    // WebSocket handler.
    let mut payload = payload;
    payload.subject = payload.subject.filter(|s| !s.is_empty());

    {
        let mut config = state.room_config.lock().unwrap();
        *config = payload.clone();
    }

    // Clear state for new room (since we have a single global room instance)
    {
        let mut p = state.participants.lock().unwrap();
        p.clear();
    }
    {
        let mut k = state.knocking_participants.lock().unwrap();
        k.clear();
    }
    {
        let mut polls = state.polls.lock().unwrap();
        polls.clear();
    }
    {
        let mut wb = state.whiteboard.lock().unwrap();
        wb.clear();
    }
    {
        let mut ch = state.chat_history.lock().unwrap();
        ch.clear();
    }
    {
        let mut br = state.breakout_rooms.lock().unwrap();
        br.clear();
    }
    {
        let mut pl = state.participant_locations.lock().unwrap();
        pl.clear();
    }
    {
        let mut v = state.shared_video_url.lock().unwrap();
        *v = None;
    }
    {
        let mut s = state.speaking_start_times.lock().unwrap();
        s.clear();
    }
    {
        let mut rc = state.remote_control_sessions.lock().unwrap();
        rc.clear();
    }
    {
        let mut pending = state.pending_remote_control_requests.lock().unwrap();
        pending.clear();
    }
    {
        let mut fb = state.feedback.lock().unwrap();
        fb.clear();
    }

    let room_id = format!("room-{}", uuid::Uuid::new_v4());

    let response = json!({
        "room_id": room_id,
        "config": payload,
        "status": "created"
    });

    (StatusCode::CREATED, Json(response)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tower::ServiceExt;

    fn build_state(admin_token: Option<String>) -> Arc<AppState> {
        let (tx, _rx) = tokio::sync::broadcast::channel(100);
        Arc::new(AppState {
            tx,
            participants: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            knocking_participants: Arc::new(
                std::sync::Mutex::new(std::collections::HashMap::new()),
            ),
            room_config: Arc::new(std::sync::Mutex::new(RoomConfig::default())),
            polls: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            whiteboard: Arc::new(std::sync::Mutex::new(Vec::new())),
            chat_history: Arc::new(std::sync::Mutex::new(Vec::new())),
            breakout_rooms: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            participant_locations: Arc::new(
                std::sync::Mutex::new(std::collections::HashMap::new()),
            ),
            shared_video_url: Arc::new(std::sync::Mutex::new(None)),
            speaking_start_times: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            feedback: Arc::new(std::sync::Mutex::new(Vec::new())),
            remote_control_sessions: Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            pending_remote_control_requests: Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
            unmute_permissions: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            camera_permissions: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            feedback_timestamps: Arc::new(Mutex::new(HashMap::new())),
            poll_seq: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            whiteboard_rev: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            admin_token,
        })
    }

    fn app(state: Arc<AppState>) -> Router {
        Router::new()
            .route("/api/rooms", post(create_room))
            .with_state(state)
    }

    fn request(body: serde_json::Value, token: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .uri("/api/rooms")
            .method("POST")
            .header("Content-Type", "application/json");
        if let Some(token) = token {
            builder = builder.header("Authorization", format!("Bearer {token}"));
        }
        builder
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }

    fn valid_body() -> serde_json::Value {
        serde_json::to_value(RoomConfig {
            e2ee_enabled: true,
            ..Default::default()
        })
        .unwrap()
    }

    #[tokio::test]
    async fn test_create_room_with_valid_token() {
        let state = build_state(Some("secret".to_string()));
        let response = app(state)
            .oneshot(request(valid_body(), Some("secret")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_create_room_without_token_is_unauthorized() {
        let state = build_state(Some("secret".to_string()));
        let response = app(state)
            .oneshot(request(valid_body(), None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_room_with_wrong_token_is_unauthorized() {
        let state = build_state(Some("secret".to_string()));
        let response = app(state)
            .oneshot(request(valid_body(), Some("guess")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_room_disabled_when_token_unset() {
        let state = build_state(None);
        let response = app(state)
            .oneshot(request(valid_body(), Some("secret")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_room_rejects_zero_max_participants() {
        let state = build_state(Some("secret".to_string()));
        let mut body = valid_body();
        body["max_participants"] = serde_json::json!(0);
        let response = app(state)
            .oneshot(request(body, Some("secret")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_room_rejects_overlong_room_name() {
        let state = build_state(Some("secret".to_string()));
        let mut body = valid_body();
        body["room_name"] = serde_json::json!("a".repeat(129));
        let response = app(state)
            .oneshot(request(body, Some("secret")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_room_accepts_lowercase_bearer_scheme() {
        let state = build_state(Some("secret".to_string()));
        let builder = Request::builder()
            .uri("/api/rooms")
            .method("POST")
            .header("Content-Type", "application/json")
            .header("Authorization", "bearer secret");
        let req = builder
            .body(Body::from(serde_json::to_string(&valid_body()).unwrap()))
            .unwrap();
        let response = app(state).oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_create_room_rejects_invalid_branding_colors() {
        for field in ["primary_color", "background_color"] {
            let state = build_state(Some("secret".to_string()));
            let mut body = valid_body();
            body["branding"][field] = serde_json::json!("url(https://evil.example/x)");
            let response = app(state)
                .oneshot(request(body, Some("secret")))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "field {field}");
        }
    }

    #[tokio::test]
    async fn test_create_room_rejects_non_https_logo() {
        let state = build_state(Some("secret".to_string()));
        let mut body = valid_body();
        body["branding"]["logo_url"] = serde_json::json!("http://evil.example/logo.png");
        let response = app(state)
            .oneshot(request(body, Some("secret")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_room_accepts_valid_branding() {
        let state = build_state(Some("secret".to_string()));
        let mut body = valid_body();
        body["branding"]["primary_color"] = serde_json::json!("#ff8800");
        body["branding"]["background_color"] = serde_json::json!("#0a0b0c");
        body["branding"]["logo_url"] = serde_json::json!("https://example.com/logo.png");
        let response = app(state)
            .oneshot(request(body, Some("secret")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
}
