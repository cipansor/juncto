use crate::AppState;
use shared::{Poll, ServerMessage};
use std::sync::Arc;

/// Maximum number of polls retained in the in-memory room state. Older polls
/// are evicted so the host cannot grow server memory without bound.
pub const MAX_POLLS: usize = 200;

pub fn create_poll(
    user_id: &str,
    mut poll: Poll,
    state: &Arc<AppState>,
) -> Result<ServerMessage, String> {
    let is_host = {
        let config = state.room_config.lock().unwrap();
        config.host_id.as_deref() == Some(user_id)
    };

    if !is_host {
        return Err("Only host can create polls".to_string());
    }

    if poll.id.is_empty() {
        poll.id = uuid::Uuid::new_v4().to_string();
    }

    {
        let mut polls = state.polls.lock().unwrap();
        polls.insert(poll.id.clone(), poll.clone());
        // Evict the oldest poll(s) once the cap is reached. HashMap iteration
        // order is unspecified, so we simply remove arbitrary entries down to
        // the limit; correctness does not depend on which poll is dropped.
        while polls.len() > MAX_POLLS {
            let Some(oldest) = polls.keys().next().cloned() else {
                break;
            };
            polls.remove(&oldest);
        }
    }

    Ok(ServerMessage::PollCreated(poll))
}

pub fn vote(
    user_id: &str,
    poll_id: String,
    option_id: u32,
    state: &Arc<AppState>,
) -> Result<ServerMessage, String> {
    let mut polls = state.polls.lock().unwrap();

    if let Some(poll) = polls.get_mut(&poll_id) {
        if poll.is_closed {
            return Err("Poll is closed".to_string());
        }

        if poll.voters.contains(user_id) {
            return Err("Already voted".to_string());
        }

        let mut found = false;
        for opt in &mut poll.options {
            if opt.id == option_id {
                opt.votes += 1;
                found = true;
                break;
            }
        }

        if !found {
            return Err("Option not found".to_string());
        }

        poll.voters.insert(user_id.to_string());

        Ok(ServerMessage::PollUpdated(poll.clone()))
    } else {
        Err("Poll not found".to_string())
    }
}

pub fn close_poll(
    user_id: &str,
    poll_id: String,
    state: &Arc<AppState>,
) -> Result<ServerMessage, String> {
    let is_host = {
        let config = state.room_config.lock().unwrap();
        config.host_id.as_deref() == Some(user_id)
    };

    if !is_host {
        return Err("Only host can close polls".to_string());
    }

    let mut polls = state.polls.lock().unwrap();
    if let Some(poll) = polls.get_mut(&poll_id) {
        poll.is_closed = true;
        Ok(ServerMessage::PollClosed(poll_id))
    } else {
        Err("Poll not found".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{PollOption, RoomConfig};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio::sync::broadcast;

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
    fn test_create_poll_host() {
        let state = create_mock_state();
        let user_id = "host";
        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(user_id.to_string());
        }

        let poll = Poll {
            id: "".to_string(),
            question: "Test?".to_string(),
            options: vec![],
            voters: std::collections::HashSet::new(),
            is_closed: false,
        };

        let res = create_poll(user_id, poll, &state);
        assert!(res.is_ok());
        if let Ok(ServerMessage::PollCreated(p)) = res {
            assert!(!p.id.is_empty());
            assert_eq!(p.question, "Test?");
        } else {
            panic!("Wrong message type");
        }
    }

    #[test]
    fn test_polls_are_bounded() {
        let state = create_mock_state();
        let user_id = "host";
        {
            let mut config = state.room_config.lock().unwrap();
            config.host_id = Some(user_id.to_string());
        }

        for i in 0..(MAX_POLLS + 5) {
            let poll = Poll {
                id: format!("poll-{i}"),
                question: format!("Q{i}"),
                options: vec![],
                voters: std::collections::HashSet::new(),
                is_closed: false,
            };
            assert!(create_poll(user_id, poll, &state).is_ok());
        }

        assert_eq!(state.polls.lock().unwrap().len(), MAX_POLLS);
    }

    #[test]
    fn test_create_poll_not_host() {
        let state = create_mock_state();
        let user_id = "guest";
        // No host set, or host is someone else

        let poll = Poll {
            id: "".to_string(),
            question: "Test?".to_string(),
            options: vec![],
            voters: std::collections::HashSet::new(),
            is_closed: false,
        };

        let res = create_poll(user_id, poll, &state);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Only host can create polls");
    }

    #[test]
    fn test_vote() {
        let state = create_mock_state();
        let poll_id = "poll1".to_string();
        let user_id = "voter1";

        {
            let mut polls = state.polls.lock().unwrap();
            polls.insert(
                poll_id.clone(),
                Poll {
                    id: poll_id.clone(),
                    question: "Q".to_string(),
                    options: vec![
                        PollOption {
                            id: 1,
                            text: "A".to_string(),
                            votes: 0,
                        },
                        PollOption {
                            id: 2,
                            text: "B".to_string(),
                            votes: 0,
                        },
                    ],
                    voters: std::collections::HashSet::new(),
                    is_closed: false,
                },
            );
        }

        let res = vote(user_id, poll_id.clone(), 1, &state);
        assert!(res.is_ok());
        if let Ok(ServerMessage::PollUpdated(p)) = res {
            assert_eq!(p.options[0].votes, 1);
            assert!(p.voters.contains(user_id));
        } else {
            panic!("Wrong message type");
        }

        // Vote again
        let res2 = vote(user_id, poll_id.clone(), 2, &state);
        assert!(res2.is_err()); // Already voted
    }

    #[test]
    fn test_vote_on_closed_poll() {
        let state = create_mock_state();
        let poll_id = "poll_closed".to_string();
        let user_id = "voter1";

        {
            let mut polls = state.polls.lock().unwrap();
            polls.insert(
                poll_id.clone(),
                Poll {
                    id: poll_id.clone(),
                    question: "Closed Q".to_string(),
                    options: vec![PollOption {
                        id: 1,
                        text: "A".to_string(),
                        votes: 0,
                    }],
                    voters: std::collections::HashSet::new(),
                    is_closed: true,
                },
            );
        }

        let res = vote(user_id, poll_id.clone(), 1, &state);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Poll is closed");
    }
}
