use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{MediaStream, MediaStreamConstraints};

/// Error names that a browser may raise when it cannot satisfy the requested
/// audio constraints. Only these justify retrying `getDisplayMedia` without
/// audio, because they are raised before the surface picker is shown.
pub(crate) fn is_audio_capability_error_name(name: &str) -> bool {
    matches!(
        name,
        "NotSupportedError" | "TypeError" | "OverconstrainedError" | "NotFoundError"
    )
}

/// Reads the `name` property of a DOM error and reports whether it is an
/// audio-capability error (see [`is_audio_capability_error_name`]).
fn is_audio_capability_error(err: &JsValue) -> bool {
    js_sys::Reflect::get(err, &"name".into())
        .ok()
        .and_then(|name| name.as_string())
        .is_some_and(|name| is_audio_capability_error_name(&name))
}

pub async fn get_user_media(
    enable_video: bool,
    enable_audio: bool,
    video_device_id: Option<String>,
    audio_device_id: Option<String>,
    video_resolution: Option<&str>,
) -> Result<MediaStream, JsValue> {
    let window = web_sys::window().ok_or(JsValue::from_str("No global window"))?;
    let navigator = window.navigator();
    let media_devices = navigator.media_devices()?;

    let constraints = MediaStreamConstraints::new();

    // Video constraints
    let video_val = if enable_video {
        let video_obj = js_sys::Object::new();
        if let Some(id) = video_device_id {
            let _ = js_sys::Reflect::set(&video_obj, &"deviceId".into(), &id.into());
        }

        if let Some(res) = video_resolution {
            if res == "hd" {
                let _ = js_sys::Reflect::set(&video_obj, &"width".into(), &1280.into());
                let _ = js_sys::Reflect::set(&video_obj, &"height".into(), &720.into());
            } else if res == "sd" {
                let _ = js_sys::Reflect::set(&video_obj, &"width".into(), &640.into());
                let _ = js_sys::Reflect::set(&video_obj, &"height".into(), &360.into());
            }
        }
        wasm_bindgen::JsValue::from(video_obj)
    } else {
        wasm_bindgen::JsValue::FALSE
    };

    constraints.set_video(&video_val);

    // Audio constraints
    let audio_val = if enable_audio {
        if let Some(id) = audio_device_id {
            let audio_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&audio_obj, &"deviceId".into(), &id.into());
            wasm_bindgen::JsValue::from(audio_obj)
        } else {
            wasm_bindgen::JsValue::TRUE
        }
    } else {
        wasm_bindgen::JsValue::FALSE
    };
    constraints.set_audio(&audio_val);

    let promise = media_devices.get_user_media_with_constraints(&constraints)?;
    let result: JsValue = JsFuture::from(promise).await?;

    result
        .dyn_into::<MediaStream>()
        .map_err(|_| JsValue::from_str("Not a MediaStream"))
}

pub async fn get_display_media() -> Result<MediaStream, JsValue> {
    let window = web_sys::window().ok_or(JsValue::from_str("No global window"))?;
    let navigator = window.navigator();
    let media_devices = navigator.media_devices()?;

    let func_val = js_sys::Reflect::get(&media_devices, &"getDisplayMedia".into())?;
    let func = func_val.dyn_into::<js_sys::Function>()?;

    // First attempt with audio: true. Some browsers/surfaces (e.g. Firefox,
    // application windows on macOS) may reject audio capture, so fall back
    // to video-only on failure rather than failing the whole screen share.
    //
    // Important: never retry on a *promise rejection*. By the time the
    // promise rejects, the browser has already shown the picker and the
    // user has interacted with it (either selecting a surface or
    // cancelling). Retrying here would force the user to interact with a
    // second picker dialog — even for capability errors like
    // `NotSupportedError` that some browsers raise asynchronously after
    // the picker is shown.
    //
    // We only retry on a *synchronous throw* from `call1`. That happens
    // before the picker is shown (e.g. the browser rejects the
    // constraints object as invalid, typically as a `TypeError`), so
    // retrying with the relaxed video-only constraints does not produce
    // a duplicate picker.

    let constraints_with_audio = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &constraints_with_audio,
        &"video".into(),
        &wasm_bindgen::JsValue::TRUE,
    );
    let _ = js_sys::Reflect::set(
        &constraints_with_audio,
        &"audio".into(),
        &wasm_bindgen::JsValue::TRUE,
    );

    let result: JsValue = match func.call1(
        &media_devices,
        &wasm_bindgen::JsValue::from(constraints_with_audio),
    ) {
        Ok(promise) => {
            // The picker has been (or will be) shown by the browser. Any
            // error from awaiting this promise — including capability
            // errors raised after surface selection — is propagated as-is
            // to avoid showing a second picker dialog on retry.
            JsFuture::from(js_sys::Promise::from(promise)).await?
        }
        Err(e) => {
            // Synchronous throw from `call1`: the picker has not been
            // shown yet, so retrying with relaxed constraints is safe.
            // Only retry on errors that look like audio-constraint
            // rejections; other synchronous errors (rare) are propagated.
            if !is_audio_capability_error(&e) {
                return Err(e);
            }
            let constraints_video_only = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &constraints_video_only,
                &"video".into(),
                &wasm_bindgen::JsValue::TRUE,
            );
            let promise = func.call1(
                &media_devices,
                &wasm_bindgen::JsValue::from(constraints_video_only),
            )?;
            JsFuture::from(js_sys::Promise::from(promise)).await?
        }
    };

    result
        .dyn_into::<MediaStream>()
        .map_err(|_| JsValue::from_str("Not a MediaStream"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treats_capability_errors_as_retryable() {
        for name in [
            "NotSupportedError",
            "TypeError",
            "OverconstrainedError",
            "NotFoundError",
        ] {
            assert!(
                is_audio_capability_error_name(name),
                "{name} should allow a video-only retry"
            );
        }
    }

    #[test]
    fn does_not_retry_on_permission_or_user_errors() {
        // Retrying these would prompt the user a second time.
        for name in [
            "NotAllowedError",
            "PermissionDeniedError",
            "AbortError",
            "InvalidStateError",
            "SecurityError",
            "",
        ] {
            assert!(
                !is_audio_capability_error_name(name),
                "{name} must not trigger a retry"
            );
        }
    }

    #[test]
    fn unknown_error_name_is_not_retried() {
        assert!(!is_audio_capability_error_name("SomethingElse"));
    }
}
