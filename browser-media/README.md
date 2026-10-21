# browser-media

Browser media capture for Rust compiled to WebAssembly.

`browser-media` wraps the Web APIs that every video, voice or recording
front-end needs and that `web-sys` exposes only as raw bindings: device
enumeration, `getUserMedia`, screen sharing, virtual backgrounds, microphone
level and voice-activity monitoring, and `MediaRecorder` download.

It is framework-agnostic, holds no global state, and releases every resource on
`Drop`. There is no equivalent module in `gloo`, so this fills a gap rather than
duplicating one.

```toml
[dependencies]
browser-media = "0.1"
```

## What it covers

| Task | Entry point |
|---|---|
| List cameras and microphones | `get_video_input_devices`, `get_audio_input_devices`, `enumerate_devices` |
| Acquire a camera/microphone stream | `get_user_media` |
| Share a screen | `get_display_media` |
| Blur or replace the camera background | `VideoProcessor` |
| Detect speech, meter the microphone, suppress noise | `AudioMonitor` |
| Record a stream to a downloadable file | `LocalRecorder` |

## Example

```rust,no_run
use browser_media::{AudioMonitor, get_audio_input_devices, get_user_media};

# async fn example() -> Result<(), wasm_bindgen::JsValue> {
let microphones = get_audio_input_devices().await?;
let device_id = microphones.first().map(|d| d.device_id.clone());

let stream = get_user_media(true, true, None, device_id, Some("hd")).await?;

let monitor = AudioMonitor::new(
    &stream,
    Box::new(|talking| {
        // Drive a "speaking" indicator.
        let _ = talking;
    }),
    None,
    None,
    true, // noise suppression
)?;
# drop(monitor);
# Ok(())
# }
```

## Behaviour worth knowing

These are the details the crate exists to get right, and the ones most likely to
be re-implemented incorrectly:

* **Speech detection survives a mute button.** `AudioMonitor` clones the audio
  track and analyses the clone. Disabling the original track would feed silence
  to the analyser, so talking-while-muted could never be detected.
* **Screen share falls back without a second picker.** `get_display_media`
  retries without audio only when `getDisplayMedia` throws *synchronously*. Once
  the promise rejects the user has already answered a picker, and retrying would
  force them to answer a second one.
* **Recordings download reliably.** The object URL is revoked on a timer rather
  than immediately after `click()`, because revoking synchronously breaks the
  download in some browsers.
* **Recording codecs are negotiated.** `LocalRecorder` picks the first supported
  entry of `video/webm;codecs=vp9,opus`, `vp8,opus`, `video/webm`, `audio/webm`
  instead of assuming VP8 support.

## Warning conditions

Two conditions are reported as events on `window`, because the sampling closure
cannot reach caller state:

| Event | Raised when |
|---|---|
| `talk_while_muted` | The user talks while muted for at least one second. |
| `noise_detected` | Sustained background noise for five seconds while not talking. |

`AudioMonitor::new` also takes an `on_no_audio` callback, invoked once after ten
seconds of silence that follow no speech at all — a likely broken microphone.

## Browser support

`get_display_media` needs `getDisplayMedia`, and `VideoProcessor` needs
`HTMLCanvasElement::captureStream`. Both are missing from older browsers and
return a `JsValue` error instead of panicking. `AudioMonitor` needs
`AudioContext`, which some browsers gate behind a user gesture.

## License

Apache-2.0. See [LICENSE](LICENSE).
