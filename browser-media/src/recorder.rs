use send_wrapper::SendWrapper;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{BlobEvent, MediaRecorder, MediaRecorderOptions, MediaStream};

/// Container/codec combinations to try, most preferred first. The first one
/// the browser reports as supported wins, so recording still works on engines
/// that lack VP9 or Opus.
pub(crate) const CANDIDATE_MIME_TYPES: &[&str] = &[
    "video/webm;codecs=vp9,opus",
    "video/webm;codecs=vp8,opus",
    "video/webm",
    "audio/webm",
];

/// Filename stem used when the caller does not supply one.
pub const DEFAULT_FILENAME_PREFIX: &str = "recording";

/// Returns the first candidate mime type accepted by `supported`.
///
/// Split out from [`pick_mime_type`] so the selection order can be tested
/// without a browser.
pub(crate) fn pick_mime_type_with(supported: impl Fn(&str) -> bool) -> Option<&'static str> {
    CANDIDATE_MIME_TYPES.iter().copied().find(|t| supported(t))
}

/// Returns the most preferred mime type the current browser supports.
fn pick_mime_type() -> Option<&'static str> {
    pick_mime_type_with(MediaRecorder::is_type_supported)
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
    /// The downloaded file is named `recording-<timestamp>.webm`; change the
    /// stem with [`LocalRecorder::set_filename_prefix`].
    pub fn new(stream: MediaStream, on_error: impl Fn(String) + 'static) -> Result<Self, JsValue> {
        let options = MediaRecorderOptions::new();
        if let Some(mime_type) = pick_mime_type() {
            options.set_mime_type(mime_type);
        }

        let recorder =
            MediaRecorder::new_with_media_stream_and_media_recorder_options(&stream, &options)?;

        let chunks: Rc<RefCell<Vec<web_sys::Blob>>> = Rc::new(RefCell::new(Vec::new()));
        let filename_prefix = Rc::new(RefCell::new(DEFAULT_FILENAME_PREFIX.to_string()));

        let chunks_clone = chunks.clone();
        let on_data_available = Closure::wrap(Box::new(move |e: BlobEvent| {
            if let Some(blob) = e.data()
                && blob.size() > 0.0
            {
                chunks_clone.borrow_mut().push(blob);
            }
        }) as Box<dyn FnMut(BlobEvent)>);

        let chunks_clone_2 = chunks.clone();
        let filename_prefix_clone = filename_prefix.clone();
        let on_stop = Closure::wrap(Box::new(move || {
            let blob_parts = js_sys::Array::new();
            for blob in chunks_clone_2.borrow().iter() {
                blob_parts.push(blob);
            }

            let property_bag = web_sys::BlobPropertyBag::new();
            property_bag.set_type("video/webm");
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
                    "{}-{}.webm",
                    filename_prefix_clone.borrow(),
                    js_sys::Date::now()
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
        let picked = pick_mime_type_with(|_| true);
        assert_eq!(picked, Some("video/webm;codecs=vp9,opus"));
    }

    #[test]
    fn falls_back_when_vp9_unsupported() {
        let picked = pick_mime_type_with(|t| t != "video/webm;codecs=vp9,opus");
        assert_eq!(picked, Some("video/webm;codecs=vp8,opus"));
    }

    #[test]
    fn falls_back_to_bare_webm() {
        let picked = pick_mime_type_with(|t| !t.contains("codecs"));
        assert_eq!(picked, Some("video/webm"));
    }

    #[test]
    fn returns_none_when_nothing_is_supported() {
        assert_eq!(pick_mime_type_with(|_| false), None);
    }

    #[test]
    fn candidates_are_ordered_from_most_to_least_preferred() {
        // vp9 before vp8, and both before the bare container.
        let vp9 = CANDIDATE_MIME_TYPES.iter().position(|t| t.contains("vp9"));
        let vp8 = CANDIDATE_MIME_TYPES.iter().position(|t| t.contains("vp8"));
        let bare = CANDIDATE_MIME_TYPES.iter().position(|t| *t == "video/webm");
        assert!(vp9 < vp8 && vp8 < bare);
    }
}
