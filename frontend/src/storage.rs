use serde::{Deserialize, Serialize};

#[allow(dead_code)]
const STORAGE_KEY: &str = "juncto_settings";

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct UserSettings {
    pub display_name: Option<String>,
    pub camera_id: Option<String>,
    pub mic_id: Option<String>,
    pub resolution: Option<String>,
    #[serde(default)]
    pub recent_rooms: Vec<String>,
    /// Stable per-browser identifier used to scope server-side rate limits
    /// (feedback). Without it every client collapses into one "anonymous"
    /// bucket and the fourth submission from anyone is rejected.
    #[serde(default)]
    pub client_id: Option<String>,
}

pub fn load_settings() -> UserSettings {
    #[cfg(not(target_arch = "wasm32"))]
    return UserSettings::default();

    #[cfg(target_arch = "wasm32")]
    {
        let window = match web_sys::window() {
            Some(w) => w,
            None => return UserSettings::default(),
        };
        let storage = window.local_storage().ok().flatten();

        if let Some(storage) = storage {
            let _key = STORAGE_KEY;
            if let Ok(Some(json)) = storage.get_item(_key) {
                return serde_json::from_str::<UserSettings>(&json).unwrap_or_default();
            }
        }
        UserSettings::default()
    }
}

pub fn save_settings(settings: &UserSettings) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = settings; // suppress unused warning

    #[cfg(target_arch = "wasm32")]
    {
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let storage = window.local_storage().ok().flatten();

        if let Some(storage) = storage
            && let Ok(json) = serde_json::to_string(settings)
        {
            let _key = STORAGE_KEY;
            let _ = storage.set_item(_key, &json);
        }
    }
}

/// Returns this browser's stable identifier, generating and persisting one on
/// first use.
pub fn client_id() -> String {
    if let Some(existing) = load_settings().client_id
        && !existing.is_empty()
    {
        return existing;
    }

    #[cfg(not(target_arch = "wasm32"))]
    let generated = "non-wasm-client".to_string();

    #[cfg(target_arch = "wasm32")]
    let generated = {
        let crypto = web_sys::window().and_then(|w| w.crypto().ok());
        match crypto {
            Some(c) => {
                let mut bytes = [0u8; 16];
                if c.get_random_values_with_u8_array(&mut bytes).is_ok() {
                    bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
                } else {
                    format!("{:?}", std::time::SystemTime::now())
                }
            }
            None => format!("{:?}", std::time::SystemTime::now()),
        }
    };

    let value = generated;
    update_setting(|s| s.client_id = Some(value.clone()));
    value
}

pub fn add_recent_room(room_id: String) {
    update_setting(move |s| {
        if let Some(pos) = s.recent_rooms.iter().position(|r| r == &room_id) {
            s.recent_rooms.remove(pos);
        }
        s.recent_rooms.insert(0, room_id);
        if s.recent_rooms.len() > 5 {
            s.recent_rooms.truncate(5);
        }
    });
}

pub fn update_setting<F>(update_fn: F)
where
    F: FnOnce(&mut UserSettings),
{
    let mut settings = load_settings();
    update_fn(&mut settings);
    save_settings(&settings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_serialization() {
        let settings = UserSettings {
            display_name: Some("Test User".to_string()),
            camera_id: Some("cam123".to_string()),
            mic_id: None,
            resolution: Some("hd".to_string()),
            recent_rooms: vec!["room1".to_string()],
            client_id: Some("cid".to_string()),
        };
        let json = serde_json::to_string(&settings).unwrap();
        let decoded: UserSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.display_name, settings.display_name);
        assert_eq!(decoded.camera_id, settings.camera_id);
        assert_eq!(decoded.recent_rooms, settings.recent_rooms);
        assert_eq!(decoded.client_id, settings.client_id);
    }

    #[test]
    fn legacy_settings_without_client_id_still_load() {
        let json = r#"{"display_name":"Old","recent_rooms":["r1"]}"#;
        let decoded: UserSettings = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.client_id, None);
        assert_eq!(decoded.recent_rooms, vec!["r1".to_string()]);
    }

    #[test]
    fn test_add_recent_room_logic() {
        let mut settings = UserSettings::default();

        let add_to_vec = |room: &str, vec: &mut Vec<String>| {
            if let Some(pos) = vec.iter().position(|r| r == room) {
                vec.remove(pos);
            }
            vec.insert(0, room.to_string());
            if vec.len() > 5 {
                vec.truncate(5);
            }
        };

        add_to_vec("room1", &mut settings.recent_rooms);
        assert_eq!(settings.recent_rooms, vec!["room1"]);

        add_to_vec("room2", &mut settings.recent_rooms);
        assert_eq!(settings.recent_rooms, vec!["room2", "room1"]);

        add_to_vec("room1", &mut settings.recent_rooms);
        assert_eq!(settings.recent_rooms, vec!["room1", "room2"]); // room1 moved to front

        for i in 3..10 {
            add_to_vec(&format!("room{}", i), &mut settings.recent_rooms);
        }
        assert_eq!(settings.recent_rooms.len(), 5);
        assert_eq!(settings.recent_rooms[0], "room9");
    }
}
