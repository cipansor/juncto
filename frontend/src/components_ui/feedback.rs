use crate::components_ui::toast::{ToastType, use_toast};
use crate::i18n::t;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::Feedback;

#[component]
pub fn FeedbackDialog(show: ReadSignal<bool>, on_close: Callback<()>) -> impl IntoView {
    let (stars, set_stars) = signal(0u8);
    let (comment, set_comment) = signal("".to_string());
    let toast = use_toast();

    let submit = move |_| {
        let s = stars.get();
        if s == 0 {
            toast.add(t("please_select_rating"), ToastType::Error);
            return;
        }
        let c = comment.get();

        let msg_submitted = t("feedback_submitted");
        let msg_error = t("feedback_error");

        spawn_local(async move {
            let feedback = Feedback {
                stars: s,
                comment: c,
                user_id: Some(crate::storage::client_id()),
            };

            let client = gloo_net::http::Request::post("/api/feedback").json(&feedback);

            match client {
                Ok(req) => match req.send().await {
                    Ok(resp) => {
                        if resp.ok() {
                            toast.add(msg_submitted, ToastType::Success);
                            on_close.run(());
                            set_stars.set(0);
                            set_comment.set("".to_string());
                        } else {
                            toast.add(msg_error.clone(), ToastType::Error);
                        }
                    }
                    Err(_) => toast.add(msg_error.clone(), ToastType::Error),
                },
                Err(_) => toast.add(msg_error, ToastType::Error),
            }
        });
    };

    view! {
        <Show when=move || show.get()>
            <div class="modal-overlay" style="position: fixed; top: 0; left: 0; width: 100%; height: 100%; background: rgba(0,0,0,0.5); display: flex; justify-content: center; align-items: center; z-index: 2000;">
                <div class="modal-content" style="width: 400px;">
                    <div class="modal-header">
                        <h3>{move || t("feedback")}</h3>
                        <button class="modal-close-btn" on:click=move |_| on_close.run(())>"×"</button>
                    </div>

                    <div style="margin-bottom: 20px; display: flex; justify-content: center; gap: 10px;">
                        <For
                            each=move || 1..=5
                            key=|i| *i
                            children=move |i| {
                                view! {
                                    <span
                                        class="feedback-star"
                                        on:click=move |_| set_stars.set(i)
                                        style=move || format!("cursor: pointer; font-size: 30px; color: {};", if stars.get() >= i { "#fbbf24" } else { "var(--text-muted)" })
                                    >
                                        "★"
                                    </span>
                                }
                            }
                        />
                    </div>

                    <textarea
                        prop:value=comment
                        on:input=move |ev| set_comment.set(event_target_value(&ev))
                        placeholder=move || t("feedback_placeholder")
                        style="width: 100%; height: 100px; margin-bottom: 20px;"
                    />

                    <div style="text-align: right;">
                        <button
                            class="submit-feedback-btn btn btn-primary"
                            on:click=submit
                        >
                            {move || t("submit")}
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
    #[ignore]
    fn test_feedback_dialog_compiles() {
        let owner = Owner::new();
        owner.set();
        let _show = RwSignal::new(true);
        let on_cancel = Callback::new(|_: ()| {});

        let show = RwSignal::new(true);
        let _view = view! {
            <FeedbackDialog show=show.read_only() on_close=on_cancel />
        };
        let _ = true;
    }
}
