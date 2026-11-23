use crate::i18n::t;
use leptos::prelude::*;

/// Thin re-export so the grid and dialog cannot drift from the server.
pub(crate) fn youtube_video_id(url: &str) -> Option<String> {
    shared::youtube_video_id(url)
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
                            <p id="shared-video-error" class="form-error">
                                "Enter a valid YouTube link, e.g. https://www.youtube.com/watch?v=dQw4w9WgXcQ"
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
