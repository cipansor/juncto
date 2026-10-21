//! Browser media capture helpers for Rust compiled to WebAssembly.
//!
//! `browser-media` wraps the Web APIs that every video, voice or recording
//! front-end needs, and that `web-sys` exposes only as raw bindings:
//!
//! | Task | Entry point |
//! |---|---|
//! | List cameras and microphones | [`get_video_input_devices`], [`get_audio_input_devices`], [`enumerate_devices`] |
//! | Acquire a stream | [`get_user_media`] |
//! | Share a screen | [`get_display_media`] |
//! | Blur or replace the camera background | [`VideoProcessor`] |
//! | Detect speech, meter the microphone, suppress noise | [`AudioMonitor`] |
//! | Record a stream to a downloadable file | [`LocalRecorder`] |
//!
//! The crate is deliberately framework-agnostic: it depends on no UI library
//! and holds no global state. Handles are released on `Drop`.
//!
//! # Example
//!
//! ```no_run
//! use browser_media::{get_audio_input_devices, get_user_media, AudioMonitor};
//!
//! # async fn example() -> Result<(), wasm_bindgen::JsValue> {
//! let microphones = get_audio_input_devices().await?;
//! let device_id = microphones.first().map(|d| d.device_id.clone());
//!
//! let stream = get_user_media(true, true, None, device_id, Some("hd")).await?;
//!
//! let monitor = AudioMonitor::new(
//!     &stream,
//!     Box::new(|talking| {
//!         // Drive a "speaking" indicator.
//!         let _ = talking;
//!     }),
//!     None,
//!     None,
//!     true, // noise suppression
//! )?;
//! # drop(monitor);
//! # Ok(())
//! # }
//! ```
//!
//! # Browser support
//!
//! [`get_display_media`] uses `getDisplayMedia`, and [`VideoProcessor`] uses
//! `HTMLCanvasElement::captureStream`; both are absent from older browsers and
//! report a `JsValue` error instead of panicking. [`AudioMonitor`] additionally
//! requires `AudioContext`, which some browsers gate behind a user gesture.

mod audio;
mod capture;
mod devices;
mod recorder;
mod video;

pub use audio::{
    AudioMonitor, NOISE_DETECTED_EVENT, SAMPLE_INTERVAL_MS, SILENCE_SAMPLES_BEFORE_NO_AUDIO,
    TALK_WHILE_MUTED_EVENT,
};
pub use capture::{get_display_media, get_user_media};
pub use devices::{
    DeviceInfo, enumerate_devices, get_audio_input_devices, get_video_input_devices,
};
pub use recorder::LocalRecorder;
pub use video::VideoProcessor;
