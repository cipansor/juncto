use crate::i18n::t;
use leptos::prelude::*;

/// Extracts the YouTube video id from the URL shapes the embed supports.
/// Returns `None` for anything that is not a YouTube link with an id, so the
/// dialog can refuse to broadcast a video that would render as a blank tile.
pub(crate) fn youtube_video_id(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return None;
    }
    let (host, rest) = trimmed
        .split_once("://")?
        .1
        .split_once('/')
        .unwrap_or((trimmed.split_once("://")?.1, ""));
    let host = host.trim_start_matches("www.");
    let is_youtube = matches!(
        host,
        "youtube.com" | "m.youtube.com" | "youtu.be" | "youtube-nocookie.com"
    );
    if !is_youtube {
        return None;
    }
    let id = if host == "youtu.be" {
        rest.split(['?', '#']).next().unwrap_or("")
    } else if let Some(after) = rest.split_once("v=").map(|(_, a)| a) {
        after.split(['&', '#']).next().unwrap_or("")
    } else if let Some(after) = rest.strip_prefix("embed/") {
        after.split(['?', '#']).next().unwrap_or("")
    } else if let Some(after) = rest.strip_prefix("shorts/") {
        after.split(['?', '#']).next().unwrap_or("")
    } else {
        ""
    };
    let valid = !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then(|| id.to_string())
}

#[component]
pub fn SharedVideoDialog(
    show: ReadSignal<bool>,
    on_close: Callback<()>,
    on_submit: Callback<String>,
) -> impl IntoView {
    let (url, set_url) = signal("".to_string());
    let (invalid, set_error) = signal(false);

    view! {
        <Show when=move || show.get()>
            <div class="modal-overlay" style="position: fixed; top: 0; left: 0; width: 100%; height: 100%; background: rgba(0,0,0,0.5); display: flex; justify-content: center; align-items: center; z-index: 1000;">
                <div class="modal-content" style="width: 400px;">
                    <div class="modal-header">
                        <h3>{move || t("share_video")}</h3>
                        <button id="close-shared-video-btn" class="modal-close-btn" on:click=move |_| on_close.run(())>"×"</button>
                    </div>

                    <div class="form-group" style="margin-bottom: 15px;">
                        <label style="display: block; margin-bottom: 5px;">{move || t("youtube_url")}</label>
                        <input
                            type="text"
                            prop:value=url
                            on:input=move |ev| {
                                set_url.set(event_target_value(&ev));
                                set_error.set(false);
                            }
                            placeholder="https://www.youtube.com/watch?v=..."
                            style="width: 100%;"
                        />
                        <Show when=move || invalid.get()>
                            <p id="shared-video-error" style="color: #dc3545; font-size: 0.85rem; margin-top: 6px;">
                                "Enter a valid YouTube link, e.g. https://www.youtube.com/watch?v=ID"
                            </p>
                        </Show>
                    </div>

                    <div style="display: flex; justify-content: flex-end; gap: 10px;">
                        <button
                            class="btn btn-secondary"
                            on:click=move |_| on_close.run(())
                        >
                            {move || t("cancel")}
                        </button>
                        <button
                            id="submit-shared-video-btn"
                            class="btn btn-primary"
                            on:click=move |_| {
                                let value = url.get();
                                if youtube_video_id(&value).is_none() {
                                    set_error.set(true);
                                    return;
                                }
                                set_error.set(false);
                                on_submit.run(value);
                                on_close.run(());
                                set_url.set("".to_string());
                            }
                        >
                            {move || t("share")}
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_video_dialog_compiles() {
        // Minimal test to ensure the component definition is valid Rust
        let _ = SharedVideoDialog;
    }

    #[test]
    fn extracts_ids_from_the_supported_youtube_url_shapes() {
        assert_eq!(
            youtube_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            youtube_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            youtube_video_id("https://youtu.be/dQw4w9WgXcQ?si=abc").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            youtube_video_id("https://www.youtube.com/embed/dQw4w9WgXcQ").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            youtube_video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ").as_deref(),
            Some("dQw4w9WgXcQ")
        );
    }

    #[test]
    fn rejects_urls_that_would_render_a_blank_tile() {
        for bad in [
            "",
            "   ",
            "not a url at all",
            "javascript:alert(1)",
            "https://example.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/watch",
            "https://www.youtube.com/watch?v=",
            "https://youtu.be/",
            "https:/www.youtube.com/watch?v=dQw4w9WgXcQ",
        ] {
            assert_eq!(
                youtube_video_id(bad),
                None,
                "expected rejection for {bad:?}"
            );
        }
    }
}
