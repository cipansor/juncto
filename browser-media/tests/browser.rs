//! Browser-run tests. These exercise the real Web APIs through `wasm-bindgen`,
//! using Chromium's fake capture devices (see `webdriver.json`).
//!
//! Run with:
//!
//! ```sh
//! CHROMEDRIVER=$HOME/.local/bin/chromedriver \
//!   cargo test -p browser-media --target wasm32-unknown-unknown
//! ```
//!
//! This target only builds for wasm, so the native `cargo test` run in CI is
//! unaffected.
#![cfg(target_arch = "wasm32")]

use browser_media::{
    AudioMonitor, LocalRecorder, VideoProcessor, enumerate_devices, get_audio_input_devices,
    get_display_media, get_user_media, get_video_input_devices,
};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .unwrap();
    });
    JsFuture::from(promise).await.unwrap();
}

fn stop_all(stream: &web_sys::MediaStream) {
    for track in stream.get_tracks().iter() {
        let track: web_sys::MediaStreamTrack = track.into();
        track.stop();
    }
}

/// Chromium's fake device backend reports a camera and a microphone.
#[wasm_bindgen_test]
async fn enumerates_a_video_and_an_audio_input() {
    let devices = enumerate_devices().await.expect("enumerate_devices failed");
    assert!(
        !devices.is_empty(),
        "expected at least one media device from the fake device backend"
    );

    let video = get_video_input_devices().await.expect("video devices");
    let audio = get_audio_input_devices().await.expect("audio devices");
    assert!(!video.is_empty(), "expected a fake camera");
    assert!(!audio.is_empty(), "expected a fake microphone");
    assert!(video.iter().all(|d| d.kind == "videoinput"));
    assert!(audio.iter().all(|d| d.kind == "audioinput"));
}

#[wasm_bindgen_test]
async fn acquires_audio_and_video_tracks() {
    let stream = get_user_media(true, true, None, None, Some("sd"))
        .await
        .expect("get_user_media failed");

    assert_eq!(
        stream.get_video_tracks().length(),
        1,
        "expected exactly one video track"
    );
    assert_eq!(
        stream.get_audio_tracks().length(),
        1,
        "expected exactly one audio track"
    );

    stop_all(&stream);
}

#[wasm_bindgen_test]
async fn honours_requested_video_resolution() {
    let stream = get_user_media(true, false, None, None, Some("sd"))
        .await
        .expect("get_user_media failed");
    let track: web_sys::MediaStreamTrack = stream.get_video_tracks().get(0).into();
    let settings = track.get_settings();
    let width = js_sys::Reflect::get(&settings, &"width".into())
        .ok()
        .and_then(|v| v.as_f64())
        .expect("width setting");
    // Fake devices honour the requested size; allow slack because some builds
    // snap to the nearest supported mode.
    assert!(
        (320.0..=960.0).contains(&width),
        "expected an SD-ish width, got {width}"
    );
    track.stop();
}

/// A video-only request must not report audio tracks.
#[wasm_bindgen_test]
async fn video_only_request_has_no_audio_track() {
    let stream = get_user_media(true, false, None, None, None)
        .await
        .expect("get_user_media failed");
    assert_eq!(stream.get_audio_tracks().length(), 0);
    stop_all(&stream);
}

/// The analyser interval must produce normalised levels.
#[wasm_bindgen_test]
async fn audio_monitor_reports_levels_in_range() {
    let stream = get_user_media(false, true, None, None, None)
        .await
        .expect("get_user_media failed");

    let observed = std::rc::Rc::new(std::cell::RefCell::new(Vec::<f64>::new()));
    let observed_clone = observed.clone();

    let monitor = AudioMonitor::new(
        &stream,
        Box::new(|_talking| {}),
        Some(Box::new(move |level| {
            observed_clone.borrow_mut().push(level);
        })),
        None,
        false,
    )
    .expect("AudioMonitor::new failed");

    sleep_ms(600).await;

    let levels = observed.borrow();
    assert!(!levels.is_empty(), "expected at least one level sample");
    assert!(
        levels.iter().all(|l| (0.0..=1.0).contains(l)),
        "levels must be normalised to 0.0..=1.0, got {levels:?}"
    );

    drop(monitor);
    stop_all(&stream);
}

/// While muted the level must read exactly 0.0, even though the isolated
/// analysis stream keeps receiving audio.
#[wasm_bindgen_test]
async fn audio_monitor_reports_zero_level_while_muted() {
    let stream = get_user_media(false, true, None, None, None)
        .await
        .expect("get_user_media failed");

    let observed = std::rc::Rc::new(std::cell::RefCell::new(Vec::<f64>::new()));
    let observed_clone = observed.clone();

    let monitor = AudioMonitor::new(
        &stream,
        Box::new(|_| {}),
        Some(Box::new(move |level| {
            observed_clone.borrow_mut().push(level);
        })),
        None,
        false,
    )
    .expect("AudioMonitor::new failed");

    monitor.set_muted(true);
    sleep_ms(400).await;

    let levels = observed.borrow();
    assert!(!levels.is_empty(), "expected level samples while muted");
    assert!(
        levels.iter().all(|l| *l == 0.0),
        "muted level must be 0.0, got {levels:?}"
    );

    drop(monitor);
    stop_all(&stream);
}

/// `VideoProcessor` returns a distinct stream carrying the processed video.
#[wasm_bindgen_test]
async fn video_processor_returns_processed_stream() {
    let stream = get_user_media(true, false, None, None, None)
        .await
        .expect("get_user_media failed");

    let (processor, processed) =
        VideoProcessor::new(&stream, "blur".to_string()).expect("VideoProcessor::new failed");

    assert_eq!(
        processed.get_video_tracks().length(),
        1,
        "processed stream should carry one video track"
    );
    assert_ne!(
        processed.id(),
        stream.id(),
        "processed stream must be a distinct MediaStream"
    );

    processor.set_mode("none".to_string());

    drop(processor);
    drop(processed);
    stop_all(&stream);
}

/// Screen share needs `getDisplayMedia`. Headless Chromium may refuse it, so an
/// explicit capability error is tolerated; anything else fails the test.
#[wasm_bindgen_test]
async fn screen_share_yields_a_video_track() {
    let stream = match get_display_media().await {
        Ok(stream) => stream,
        Err(e) => {
            let name = js_sys::Reflect::get(&e, &"name".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            assert!(
                matches!(name.as_str(), "NotAllowedError" | "NotSupportedError"),
                "unexpected getDisplayMedia failure: {name} ({e:?})"
            );
            return;
        }
    };

    assert!(
        stream.get_video_tracks().length() > 0,
        "screen share must yield a video track"
    );
    stop_all(&stream);
}

/// A recorder must start, record and stop without reporting errors, and the
/// asynchronous `onstop` download must actually run. The test must not drop the
/// recorder immediately after `stop()`, or the `onstop` handler is torn down
/// before it can assemble and download the recording.
#[wasm_bindgen_test]
async fn recorder_starts_and_stops_cleanly() {
    let stream = get_user_media(true, true, None, None, None)
        .await
        .expect("get_user_media failed");

    let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let errors_clone = errors.clone();

    let recorder = LocalRecorder::new(stream.clone(), move |msg| {
        errors_clone.borrow_mut().push(msg);
    })
    .expect("LocalRecorder::new failed");

    // A stream with a video track must record in a video container.
    assert!(
        recorder.mime_type().starts_with("video/"),
        "expected a video container, got {:?}",
        recorder.mime_type()
    );

    recorder.set_filename_prefix("browser-media-test");
    sleep_ms(300).await;
    recorder.stop();

    // Let the `onstop` handler run before the recorder is dropped, so the
    // download path is exercised rather than skipped.
    sleep_ms(1500).await;

    assert!(
        errors.borrow().is_empty(),
        "recorder reported errors: {:?}",
        errors.borrow()
    );

    drop(recorder);
    stop_all(&stream);
}

/// An audio-only stream must record in an audio container. Handing it a video
/// mime type makes browsers reject the configuration even when `audio/webm`
/// would work.
#[wasm_bindgen_test]
async fn audio_only_recorder_uses_an_audio_container() {
    let stream = get_user_media(false, true, None, None, None)
        .await
        .expect("get_user_media failed");
    assert_eq!(stream.get_video_tracks().length(), 0);

    let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let errors_clone = errors.clone();

    let recorder = LocalRecorder::new(stream.clone(), move |msg| {
        errors_clone.borrow_mut().push(msg);
    })
    .expect("LocalRecorder::new failed for an audio-only stream");

    let mime = recorder.mime_type();
    assert!(
        mime.starts_with("audio/"),
        "audio-only stream must record as audio, got {mime:?}"
    );

    sleep_ms(300).await;
    recorder.stop();
    // Keep the recorder alive so `onstop` runs, then let it settle.
    sleep_ms(1500).await;

    assert!(
        errors.borrow().is_empty(),
        "recorder reported errors: {:?}",
        errors.borrow()
    );

    drop(recorder);
    stop_all(&stream);
}

/// The downloaded blob must carry the format the recorder actually produced.
/// An audio-only recording labelled `video/webm` is misclassified by
/// MIME-sensitive players.
#[wasm_bindgen_test]
async fn downloaded_blob_is_labelled_with_the_recorded_type() {
    let stream = get_user_media(false, true, None, None, None)
        .await
        .expect("get_user_media failed");

    // Intercept the blob handed to the download path.
    let url_ctor = js_sys::Reflect::get(&js_sys::global(), &"URL".into()).expect("URL global");
    let real_create =
        js_sys::Reflect::get(&url_ctor, &"createObjectURL".into()).expect("URL.createObjectURL");
    let spy = js_sys::Function::new_with_args(
        "real",
        "return function(blob) { globalThis.__bm_blob_type = blob && blob.type; return real.call(this, blob); };",
    )
    .call1(&JsValue::NULL, &real_create)
    .expect("could not build createObjectURL spy");
    js_sys::Reflect::set(&url_ctor, &"createObjectURL".into(), &spy).expect("install spy");

    let recorder = LocalRecorder::new(stream.clone(), |_| {}).expect("LocalRecorder::new failed");
    let recorded_mime = recorder.mime_type();
    assert!(
        !recorded_mime.is_empty(),
        "the mime type must be known once recording has started"
    );

    sleep_ms(300).await;
    recorder.stop();
    // Keep the recorder alive so `onstop` assembles and downloads the blob.
    sleep_ms(1500).await;

    let blob_type = js_sys::Reflect::get(&js_sys::global(), &"__bm_blob_type".into())
        .ok()
        .and_then(|v| v.as_string());

    // Restore before asserting so a failure cannot leave the stub installed.
    let _ = js_sys::Reflect::set(&url_ctor, &"createObjectURL".into(), &real_create);
    let _ = js_sys::Reflect::delete_property(
        js_sys::global().unchecked_ref::<js_sys::Object>(),
        &"__bm_blob_type".into(),
    );

    let blob_type = blob_type.expect("no blob reached URL.createObjectURL");
    assert!(
        blob_type.starts_with("audio/"),
        "blob labelled {blob_type:?} but the recorder produced {recorded_mime:?}"
    );

    drop(recorder);
    stop_all(&stream);
}

/// Removing `captureStream` must fail construction without leaving a timer
/// behind. The drawing interval is registered after `captureStream` succeeds,
/// so a failed construction cannot strand a callback the browser keeps firing
/// every 33 ms.
#[wasm_bindgen_test]
async fn video_processor_failure_leaves_no_timer_running() {
    let stream = get_user_media(true, false, None, None, None)
        .await
        .expect("get_user_media failed");

    let canvas_ctor = js_sys::Reflect::get(&js_sys::global(), &"HTMLCanvasElement".into())
        .expect("HTMLCanvasElement global");
    let canvas_proto = js_sys::Reflect::get(&canvas_ctor, &"prototype".into())
        .expect("HTMLCanvasElement.prototype");
    let original = js_sys::Reflect::get(&canvas_proto, &"captureStream".into()).ok();
    assert!(
        original.as_ref().map(|f| f.is_function()).unwrap_or(false),
        "test assumes this browser has captureStream"
    );

    // Count timers scheduled while captureStream is unavailable.
    let real_set_interval =
        js_sys::Reflect::get(&js_sys::global(), &"setInterval".into()).expect("global setInterval");
    let spy = js_sys::Function::new_with_args(
        "real",
        "return function() { globalThis.__bm_interval_count = (globalThis.__bm_interval_count||0)+1; return real.apply(this, arguments); }",
    )
    .call1(&JsValue::NULL, &real_set_interval)
    .expect("could not build setInterval spy");
    // In a browser `window` is `globalThis`, so this also replaces
    // `window.setInterval`, which is what web-sys calls.
    js_sys::Reflect::set(&js_sys::global(), &"setInterval".into(), &spy).expect("install spy");

    // Simulate a browser without captureStream.
    let _ = js_sys::Reflect::delete_property(
        canvas_proto.unchecked_ref::<js_sys::Object>(),
        &"captureStream".into(),
    );

    let result = VideoProcessor::new(&stream, "blur".to_string());
    assert!(
        result.is_err(),
        "construction must fail when captureStream is missing"
    );

    // Restore the real API before asserting, so a failure cannot leak the stub.
    let _ = js_sys::Reflect::set(&js_sys::global(), &"setInterval".into(), &real_set_interval);
    if let Some(orig) = original {
        let _ = js_sys::Reflect::set(&canvas_proto, &"captureStream".into(), &orig);
    }

    let scheduled = js_sys::Reflect::get(&js_sys::global(), &"__bm_interval_count".into())
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let _ = js_sys::Reflect::delete_property(&js_sys::global(), &"__bm_interval_count".into());

    assert_eq!(
        scheduled, 0.0,
        "a failed VideoProcessor::new must not schedule a drawing timer"
    );

    stop_all(&stream);
}
