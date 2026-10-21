use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{MediaDeviceInfo, MediaDeviceKind};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub label: String,
    pub kind: String,
}

pub async fn enumerate_devices() -> Result<Vec<MediaDeviceInfo>, JsValue> {
    let window = web_sys::window().ok_or(JsValue::from_str("No global window"))?;
    let navigator = window.navigator();
    let media_devices = navigator.media_devices()?;

    let promise = media_devices.enumerate_devices()?;
    let result: JsValue = JsFuture::from(promise).await?;

    let array: js_sys::Array = result.dyn_into()?;
    let mut devices = Vec::new();

    for i in 0..array.length() {
        let val = array.get(i);
        if let Ok(device) = val.dyn_into::<MediaDeviceInfo>() {
            devices.push(device);
        }
    }

    Ok(devices)
}

pub async fn get_video_input_devices() -> Result<Vec<DeviceInfo>, JsValue> {
    let devices = enumerate_devices().await?;
    let mut result = Vec::new();
    for device in devices {
        if device.kind() == MediaDeviceKind::Videoinput {
            result.push(DeviceInfo {
                device_id: device.device_id(),
                label: device.label(),
                kind: "videoinput".to_string(),
            });
        }
    }
    Ok(result)
}

pub async fn get_audio_input_devices() -> Result<Vec<DeviceInfo>, JsValue> {
    let devices = enumerate_devices().await?;
    let mut result = Vec::new();
    for device in devices {
        if device.kind() == MediaDeviceKind::Audioinput {
            result.push(DeviceInfo {
                device_id: device.device_id(),
                label: device.label(),
                kind: "audioinput".to_string(),
            });
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_info_round_trips_through_json() {
        let device = DeviceInfo {
            device_id: "test-id".to_string(),
            label: "Test Device".to_string(),
            kind: "videoinput".to_string(),
        };

        let json = serde_json::to_string(&device).expect("Failed to serialize");
        assert!(json.contains("test-id"));
        assert!(json.contains("Test Device"));
        assert!(json.contains("videoinput"));

        let deserialized: DeviceInfo = serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(deserialized, device);
    }
}
