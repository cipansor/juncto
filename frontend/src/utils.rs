/// Starts playback and swallows the rejection.
///
/// `play()` returns a promise that rejects whenever the element is detached or
/// re-sourced before playback begins (a routine race when a stream is replaced
/// or the grid re-renders). The rejection is harmless, but leaving it unhandled
/// surfaces as a `pageerror` in the console, which hides real errors.
pub fn play_quietly(video: &web_sys::HtmlMediaElement) {
    if let Ok(promise) = video.play() {
        wasm_bindgen_futures::spawn_local(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        });
    }
}

pub fn create_room_url(name: &str) -> String {
    let encoded_name = urlencoding::encode(name);
    format!("/room/{}", encoded_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_room_url() {
        assert_eq!(create_room_url("My Room"), "/room/My%20Room");
        assert_eq!(create_room_url("Test"), "/room/Test");
    }
}
