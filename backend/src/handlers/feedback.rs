use crate::AppState;
use axum::{
    extract::{ConnectInfo, FromRequestParts, Json, State},
    http::{StatusCode, request::Parts},
    response::IntoResponse,
};
use shared::Feedback;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

/// Peer address when the server was built with connect info, `None` otherwise
/// (unit tests drive the router with `oneshot` and have no socket).
pub struct PeerAddr(pub Option<IpAddr>);

impl<S: Send + Sync> FromRequestParts<S> for PeerAddr {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(PeerAddr(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ConnectInfo(addr)| addr.ip()),
        ))
    }
}

const WINDOW: std::time::Duration = std::time::Duration::from_secs(600);
const PER_CLIENT_LIMIT: usize = 3;
// The client identifier is self-reported, so a caller can rotate it at will.
// The address is not, so it gets a looser ceiling that still bounds abuse.
// It has to stay well above the per-client limit because a whole office or a
// mobile carrier sits behind one address.
const PER_ADDRESS_LIMIT: usize = 100;

/// Drops timestamps outside the rate-limit window and returns how many remain.
fn prune_and_count(timestamps: &mut Vec<std::time::Instant>, now: std::time::Instant) -> usize {
    timestamps.retain(|&ts| now.duration_since(ts) < WINDOW);
    timestamps.len()
}

pub async fn submit_feedback(
    State(state): State<Arc<AppState>>,
    PeerAddr(peer_ip): PeerAddr,
    Json(payload): Json<Feedback>,
) -> impl IntoResponse {
    let client_id = payload
        .user_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| "anonymous".to_string());
    let address = peer_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let now = std::time::Instant::now();

    let client_key = format!("client:{client_id}");
    let address_key = format!("addr:{address}");
    {
        let mut timestamps = state.feedback_timestamps.lock().unwrap();

        let over_limit = prune_and_count(timestamps.entry(client_key.clone()).or_default(), now)
            >= PER_CLIENT_LIMIT
            || prune_and_count(timestamps.entry(address_key.clone()).or_default(), now)
                >= PER_ADDRESS_LIMIT;
        if over_limit {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Too many feedback submissions. Please try again later.",
            )
                .into_response();
        }

        timestamps.entry(client_key).or_default().push(now);
        timestamps.entry(address_key).or_default().push(now);
    }

    // Validate stars (Bug 1)
    if payload.stars < 1 || payload.stars > 5 {
        return (StatusCode::BAD_REQUEST, "Stars must be between 1 and 5").into_response();
    }

    let mut feedback_store = state.feedback.lock().unwrap();

    // Bound storage (Bug 2)
    if feedback_store.len() >= 1000 {
        feedback_store.remove(0);
    }

    feedback_store.push(payload);

    (StatusCode::OK, "Feedback received").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::post;
    use shared::RoomConfig;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    fn mock_state() -> Arc<AppState> {
        let (tx, _rx) = tokio::sync::broadcast::channel(10);
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

    #[tokio::test]
    async fn test_submit_feedback() {
        let (tx, _rx) = tokio::sync::broadcast::channel(10);
        let app_state = Arc::new(AppState {
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
        });

        let app = Router::new()
            .route("/api/feedback", post(submit_feedback))
            .with_state(app_state.clone());

        let feedback = Feedback {
            stars: 5,
            comment: "Great app!".to_string(),
            user_id: Some("user123".to_string()),
        };

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/feedback")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&feedback).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let stored = app_state.feedback.lock().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].stars, 5);
    }

    fn post_feedback(
        app: Router,
        user_id: Option<&str>,
        peer: Option<std::net::IpAddr>,
    ) -> impl std::future::Future<Output = StatusCode> {
        let payload = Feedback {
            stars: 4,
            comment: "ok".to_string(),
            user_id: user_id.map(|u| u.to_string()),
        };
        let mut builder = Request::builder()
            .method("POST")
            .uri("/api/feedback")
            .header("Content-Type", "application/json");
        if let Some(ip) = peer {
            builder = builder.extension(ConnectInfo(SocketAddr::new(ip, 4242)));
        }
        let request = builder
            .body(Body::from(serde_json::to_string(&payload).unwrap()))
            .unwrap();
        async move { app.oneshot(request).await.unwrap().status() }
    }

    #[tokio::test]
    async fn distinct_clients_get_their_own_feedback_budget() {
        let app_state = mock_state();
        let app = Router::new()
            .route("/api/feedback", post(submit_feedback))
            .with_state(app_state);

        for _ in 0..3 {
            assert_eq!(
                post_feedback(app.clone(), Some("client-a"), None).await,
                StatusCode::OK
            );
        }
        // client-a is exhausted...
        assert_eq!(
            post_feedback(app.clone(), Some("client-a"), None).await,
            StatusCode::TOO_MANY_REQUESTS
        );
        // ...but a different browser still has its own budget, which is the bug
        // that made the fourth submission in a room fail for everybody.
        assert_eq!(
            post_feedback(app.clone(), Some("client-b"), None).await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn a_rotated_client_id_still_runs_into_the_address_ceiling() {
        let app_state = mock_state();
        let app = Router::new()
            .route("/api/feedback", post(submit_feedback))
            .with_state(app_state);

        let ip: std::net::IpAddr = "203.0.113.7".parse().unwrap();
        for i in 0..PER_ADDRESS_LIMIT {
            assert_eq!(
                post_feedback(app.clone(), Some(&format!("rotated-{i}")), Some(ip)).await,
                StatusCode::OK
            );
        }
        assert_eq!(
            post_feedback(app.clone(), Some("fresh-id"), Some(ip)).await,
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[tokio::test]
    async fn test_submit_feedback_invalid_stars() {
        let (tx, _rx) = tokio::sync::broadcast::channel(10);
        let app_state = Arc::new(AppState {
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
        });

        let app = Router::new()
            .route("/api/feedback", post(submit_feedback))
            .with_state(app_state.clone());

        let feedback = Feedback {
            stars: 6,
            comment: "Too good!".to_string(),
            user_id: None,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/feedback")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&feedback).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let stored = app_state.feedback.lock().unwrap();
        assert!(stored.is_empty());
    }
}
