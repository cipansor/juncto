use send_wrapper::SendWrapper;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{BlobEvent, MediaRecorder, MediaRecorderOptions, MediaStream};

/// Video container/codec combinations to try, most preferred first. The first
/// one the browser reports as supported wins, so recording still works on
/// engines that lack VP9 or Opus.
const VIDEO_MIME_TYPES: &[&str] = &[
    "video/webm;codecs=vp9,opus",
    "video/webm;codecs=vp8,opus",
    "video/webm",
];

/// Audio-only container/codec combinations to try, most preferred first.
const AUDIO_MIME_TYPES: &[&str] = &["audio/webm;codecs=opus", "audio/webm"];

/// Filename stem used when the caller does not supply one.
pub const DEFAULT_FILENAME_PREFIX: &str = "recording";

/// Returns the first candidate mime type for `has_video` accepted by `supported`.
///
/// `MediaRecorder.isTypeSupported` answers for a mime type in isolation, not for
/// a particular stream: a browser can report a video type as supported and then
/// refuse it for an audio-only stream. So the candidate list is selected from
/// the stream's own track composition instead of always preferring video, which
/// would leave an otherwise usable `audio/webm` unused.
///
/// Split out from the call site so the selection order can be tested without a
/// browser.
pub(crate) fn pick_mime_type_with(
    supported: impl Fn(&str) -> bool,
    has_video: bool,
) -> Option<&'static str> {
    let candidates = if has_video {
        VIDEO_MIME_TYPES
    } else {
        AUDIO_MIME_TYPES
    };
    candidates.iter().copied().find(|t| supported(t))
}

/// File extension implied by a mime type, e.g. `audio/webm;codecs=opus` -> `webm`.
fn extension_for(mime_type: &str) -> &str {
    mime_type
        .split(';')
        .next()
        .unwrap_or_default()
        .split('/')
        .nth(1)
        .filter(|s| !s.is_empty())
        .unwrap_or("webm")
}

/// Container to label a finished recording with, given the type the recorder
/// settled on and the type the browser stamped on the produced chunks. `None`
/// means neither gave evidence, and the caller must not claim a container.
fn resolve_recorded_mime(settled: Option<String>, chunk_type: Option<String>) -> Option<String> {
    // Each source must be filtered before falling through, so an empty settled
    // type still lets the chunk type have its say.
    settled
        .filter(|t| !t.is_empty())
        .or(chunk_type.filter(|t| !t.is_empty()))
}

/// Extension for the downloaded file. Falls back to `bin` when no container
/// could be determined, so the name never claims a format we did not verify.
fn download_extension(recorded: Option<&str>) -> &str {
    match recorded {
        Some(m) => extension_for(m),
        None => "bin",
    }
}

/// Records a [`MediaStream`] and, on stop, downloads the result as a file.
///
/// Dropping the recorder stops an in-flight recording; the `onstop` handler
/// then assembles and downloads what was captured. Errors raised by the
/// underlying `MediaRecorder` are reported through the closure passed to
/// [`LocalRecorder::new`].
pub struct LocalRecorder {
    recorder: SendWrapper<MediaRecorder>,
    filename_prefix: Rc<RefCell<String>>,
    _on_data_available: SendWrapper<Closure<dyn FnMut(BlobEvent)>>,
    _on_stop: SendWrapper<Closure<dyn FnMut()>>,
    _on_error: SendWrapper<Closure<dyn FnMut(web_sys::Event)>>,
}

impl LocalRecorder {
    /// Starts recording `stream`, calling `on_error` with a message if the
    /// browser reports a recording error.
    ///
    /// The mime type is chosen from the stream's own tracks, so an audio-only
    /// stream records as `audio/webm` rather than being handed a video codec it
    /// cannot use. The container is confirmed from the recorder after it starts,
    /// so the blob label and the file extension describe the format that was
    /// really produced, even when the browser picks its own. The downloaded file
    /// is named `recording-<timestamp>.<ext>`; change the stem with
    /// [`LocalRecorder::set_filename_prefix`].
    pub fn new(stream: MediaStream, on_error: impl Fn(String) + 'static) -> Result<Self, JsValue> {
        // The stream's own tracks decide which candidates make sense: preferring
        // a video type for a microphone-only stream makes the browser reject the
        // configuration even when it supports `audio/webm`.
        let has_video = stream.get_video_tracks().length() > 0;
        let requested_mime = pick_mime_type_with(MediaRecorder::is_type_supported, has_video);

        let options = MediaRecorderOptions::new();
        if let Some(mime_type) = requested_mime {
            options.set_mime_type(mime_type);
        }

        let recorder =
            MediaRecorder::new_with_media_stream_and_media_recorder_options(&stream, &options)?;

        let chunks: Rc<RefCell<Vec<web_sys::Blob>>> = Rc::new(RefCell::new(Vec::new()));
        let filename_prefix = Rc::new(RefCell::new(DEFAULT_FILENAME_PREFIX.to_string()));
        // The container is only known once recording has started, because
        // `MediaRecorder.mimeType` is empty until then. `None` means no evidence
        // yet, and while it stays `None` no container is claimed.
        let recorded_mime: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        let chunks_clone = chunks.clone();
        let recorded_mime_data = recorded_mime.clone();
        let on_data_available = Closure::wrap(Box::new(move |e: BlobEvent| {
            if let Some(blob) = e.data()
                && blob.size() > 0.0
            {
                // Last resort: some browsers leave `mimeType` empty but still
                // stamp the produced chunks with the container they chose.
                if recorded_mime_data.borrow().is_none() {
                    *recorded_mime_data.borrow_mut() =
                        resolve_recorded_mime(None, Some(blob.type_()));
                }
                chunks_clone.borrow_mut().push(blob);
            }
        }) as Box<dyn FnMut(BlobEvent)>);

        let chunks_clone_2 = chunks.clone();
        let filename_prefix_clone = filename_prefix.clone();
        let recorded_mime_stop = recorded_mime.clone();
        let on_stop = Closure::wrap(Box::new(move || {
            let blob_parts = js_sys::Array::new();
            for blob in chunks_clone_2.borrow().iter() {
                blob_parts.push(blob);
            }

            // Resolve the container at stop time: this is the first moment the
            // type is authoritative, and it also lets the chunks have their say
            // for browsers that leave `mimeType` empty.
            let recorded = recorded_mime_stop.borrow().clone();
            let extension = download_extension(recorded.as_deref());

            let property_bag = web_sys::BlobPropertyBag::new();
            // Only claim a type when there is evidence for one.
            if let Some(mime_type) = recorded.as_deref() {
                property_bag.set_type(mime_type);
            }
            if let Ok(blob) =
                web_sys::Blob::new_with_blob_sequence_and_options(&blob_parts, &property_bag)
                && let Some(window) = web_sys::window()
                && let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob)
            {
                let document = window.document().unwrap();
                let a = document
                    .create_element("a")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlAnchorElement>()
                    .unwrap();
                a.set_href(&url);
                a.set_download(&format!(
                    "{}-{}.{}",
                    filename_prefix_clone.borrow(),
                    js_sys::Date::now(),
                    extension
                ));
                a.click();
                // Delay revoking the object URL so the browser has time
                // to fully initiate the download. Revoking synchronously
                // after click() can cause download failures in some
                // browser environments.
                let url_to_revoke = url;
                let revoke_cb = Closure::once(move || {
                    let _ = web_sys::Url::revoke_object_url(&url_to_revoke);
                });
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    revoke_cb.as_ref().unchecked_ref(),
                    1000,
                );
                revoke_cb.forget();
            }
            chunks_clone_2.borrow_mut().clear();
        }) as Box<dyn FnMut()>);

        let on_error_cb = Closure::wrap(Box::new(move |e: web_sys::Event| {
            let msg = js_sys::Reflect::get(&e, &JsValue::from_str("message"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "Unknown MediaRecorder error".to_string());
            (on_error)(msg);
        }) as Box<dyn FnMut(web_sys::Event)>);

        recorder.set_ondataavailable(Some(on_data_available.as_ref().unchecked_ref()));
        recorder.set_onstop(Some(on_stop.as_ref().unchecked_ref()));
        let error_target: &web_sys::EventTarget = recorder.unchecked_ref();
        let _ = error_target
            .add_event_listener_with_callback("error", on_error_cb.as_ref().unchecked_ref());

        recorder.start_with_time_slice(5000)?; // 5s slices

        // Read the effective type only after `start()`, because `mimeType` is
        // empty before then. This is what makes the blob label and the file
        // extension reflect the container the browser really produced instead of
        // a guess; when the browser reports nothing, the value stays `None` and
        // the chunks get their say in `on_data_available`.
        if let Some(settled) = resolve_recorded_mime(Some(recorder.mime_type()), None) {
            *recorded_mime.borrow_mut() = Some(settled);
        }

        Ok(Self {
            recorder: SendWrapper::new(recorder),
            filename_prefix,
            _on_data_available: SendWrapper::new(on_data_available),
            _on_stop: SendWrapper::new(on_stop),
            _on_error: SendWrapper::new(on_error_cb),
        })
    }

    /// Sets the stem of the downloaded filename, e.g. `"standup"` produces
    /// `standup-1712345678901.webm`.
    pub fn set_filename_prefix(&self, prefix: impl Into<String>) {
        *self.filename_prefix.borrow_mut() = prefix.into();
    }

    /// The mime type the browser actually records with, as reported by
    /// `MediaRecorder.mimeType`. Empty until recording has started, after which
    /// it reflects the real container even when the browser ignored the
    /// requested one. This is the type used for the downloaded blob.
    pub fn mime_type(&self) -> String {
        self.recorder.mime_type()
    }

    /// Stops recording. The `onstop` handler downloads the captured data.
    pub fn stop(&self) {
        let _ = self.recorder.stop();
    }
}

impl Drop for LocalRecorder {
    fn drop(&mut self) {
        // Only call stop() if the recorder is still active. Calling stop() on
        // an already-stopped MediaRecorder throws a DOMException.
        if self.recorder.state() == web_sys::RecordingState::Recording
            || self.recorder.state() == web_sys::RecordingState::Paused
        {
            let _ = self.recorder.stop();
        }
        // Remove the error event listener added via addEventListener so
        // the browser doesn't try to invoke the dropped Closure.
        let error_target: &web_sys::EventTarget = self.recorder.unchecked_ref();
        let _ = error_target
            .remove_event_listener_with_callback("error", self._on_error.as_ref().unchecked_ref());
        // Clear property-based handlers for the same reason.
        self.recorder.set_ondataavailable(None);
        self.recorder.set_onstop(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_vp9_when_supported() {
        let picked = pick_mime_type_with(|_| true, true);
        assert_eq!(picked, Some("video/webm;codecs=vp9,opus"));
    }

    #[test]
    fn falls_back_when_vp9_unsupported() {
        let picked = pick_mime_type_with(|t| t != "video/webm;codecs=vp9,opus", true);
        assert_eq!(picked, Some("video/webm;codecs=vp8,opus"));
    }

    #[test]
    fn falls_back_to_bare_webm() {
        let picked = pick_mime_type_with(|t| !t.contains("codecs"), true);
        assert_eq!(picked, Some("video/webm"));
    }

    #[test]
    fn returns_none_when_nothing_is_supported() {
        assert_eq!(pick_mime_type_with(|_| false, true), None);
        assert_eq!(pick_mime_type_with(|_| false, false), None);
    }

    #[test]
    fn candidates_are_ordered_from_most_to_least_preferred() {
        // vp9 before vp8, and both before the bare container.
        let vp9 = VIDEO_MIME_TYPES.iter().position(|t| t.contains("vp9"));
        let vp8 = VIDEO_MIME_TYPES.iter().position(|t| t.contains("vp8"));
        let bare = VIDEO_MIME_TYPES.iter().position(|t| *t == "video/webm");
        assert!(vp9 < vp8 && vp8 < bare);
    }

    /// An audio-only stream must never be handed a video mime type, even when
    /// the browser claims to support one: `isTypeSupported` is asked about the
    /// type in isolation, not about this stream.
    #[test]
    fn audio_only_stream_never_selects_a_video_type() {
        let picked = pick_mime_type_with(|_| true, false);
        assert_eq!(picked, Some("audio/webm;codecs=opus"));

        // Even with video types "supported", an audio-only stream must not pick
        // one.
        assert!(!picked.unwrap().starts_with("video/"));
    }

    #[test]
    fn audio_only_falls_back_to_bare_audio_webm() {
        let picked = pick_mime_type_with(|t| t == "audio/webm", false);
        assert_eq!(picked, Some("audio/webm"));
    }

    #[test]
    fn audio_only_does_not_use_video_candidates() {
        // A browser that only reports video types as supported must yield None
        // for an audio-only stream rather than a video type.
        let picked = pick_mime_type_with(|t| t.starts_with("video/"), false);
        assert_eq!(picked, None);
    }

    #[test]
    fn extension_follows_the_container() {
        assert_eq!(extension_for("video/webm;codecs=vp9,opus"), "webm");
        assert_eq!(extension_for("audio/webm;codecs=opus"), "webm");
        assert_eq!(extension_for("audio/webm"), "webm");
        assert_eq!(extension_for("video/mp4"), "mp4");
        // A malformed type must not produce an empty extension.
        assert_eq!(extension_for(""), "webm");
    }

    #[test]
    fn settled_type_wins_over_the_chunk_type() {
        assert_eq!(
            resolve_recorded_mime(Some("audio/mp4".into()), Some("video/webm".into())),
            Some("audio/mp4".to_string())
        );
    }

    #[test]
    fn chunk_type_is_used_when_the_recorder_reports_nothing() {
        assert_eq!(
            resolve_recorded_mime(Some(String::new()), Some("audio/mp4".into())),
            Some("audio/mp4".to_string())
        );
    }

    /// With no evidence from either source the container must stay unknown, so
    /// the caller does not label an MP4 recording as `video/webm`.
    #[test]
    fn no_evidence_yields_no_container() {
        assert_eq!(resolve_recorded_mime(None, None), None);
        assert_eq!(resolve_recorded_mime(Some(String::new()), None), None);
        assert_eq!(
            resolve_recorded_mime(Some(String::new()), Some(String::new())),
            None
        );
    }

    /// An unknown container must not be presented as a WebM file.
    #[test]
    fn unknown_container_downloads_as_bin() {
        assert_eq!(download_extension(None), "bin");
        assert_eq!(download_extension(Some("audio/mp4")), "mp4");
        assert_eq!(download_extension(Some("audio/webm;codecs=opus")), "webm");
    }
}
