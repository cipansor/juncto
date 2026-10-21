use leptos::prelude::*;
use shared::{Poll, PollOption};

#[component]
pub fn PollsDialog(
    show: ReadSignal<bool>,
    polls: ReadSignal<Vec<Poll>>,
    is_host: Signal<bool>,
    on_close: Callback<()>,
    on_create_poll: Callback<Poll>,
    on_vote: Callback<(String, u32)>, // poll_id, option_id
    on_close_poll: Callback<String>,
) -> impl IntoView {
    let (active_tab, set_active_tab) = signal("active");

    // Create Poll State
    let (question, set_question) = signal("".to_string());
    let (option1, set_option1) = signal("".to_string());
    let (option2, set_option2) = signal("".to_string());

    let create = move |_| {
        let q = question.get();
        let o1 = option1.get();
        let o2 = option2.get();

        if !q.is_empty() && !o1.is_empty() && !o2.is_empty() {
            let poll = Poll {
                id: "".to_string(), // Backend assigns ID
                question: q,
                options: vec![
                    PollOption {
                        id: 0,
                        text: o1,
                        votes: 0,
                    },
                    PollOption {
                        id: 1,
                        text: o2,
                        votes: 0,
                    },
                ],
                voters: std::collections::HashSet::new(),
                is_closed: false,
                created_seq: 0,
            };
            on_create_poll.run(poll);
            // Reset and switch to active
            set_question.set("".to_string());
            set_option1.set("".to_string());
            set_option2.set("".to_string());
            set_active_tab.set("active");
        }
    };

    let active_polls = Memo::new(move |_| {
        polls
            .get()
            .into_iter()
            .filter(|p| !p.is_closed)
            .collect::<Vec<_>>()
    });

    let history_polls = Memo::new(move |_| {
        polls
            .get()
            .into_iter()
            .filter(|p| p.is_closed)
            .collect::<Vec<_>>()
    });

    view! {
        <Show when=move || show.get()>
            <div class="modal-overlay">
                <div class="modal-content">
                    <div class="modal-header">
                        <h3 class="modal-title">"📊 Polls"</h3>
                        <button id="close-polls-btn" class="modal-close-btn" on:click=move |_| on_close.run(())>"✕"</button>
                    </div>

                    <div class="tabs modal-tabs">
                        <button
                            class=move || format!("modal-tab-btn {}", if active_tab.get() == "active" { "active" } else { "" })
                            on:click=move |_| set_active_tab.set("active")
                        >
                            "Active Polls"
                        </button>
                        <button
                            class=move || format!("modal-tab-btn {}", if active_tab.get() == "history" { "active" } else { "" })
                            on:click=move |_| set_active_tab.set("history")
                        >
                            "History"
                        </button>
                        <Show when=move || is_host.get()>
                            <button
                                class=move || format!("modal-tab-btn {}", if active_tab.get() == "create" { "active" } else { "" })
                                on:click=move |_| set_active_tab.set("create")
                            >
                                "Create Poll"
                            </button>
                        </Show>
                    </div>

                    <div class="tab-content modal-body custom-scrollbar">
                        <Show when=move || active_tab.get() == "active">
                            <div class="polls-list">
                                <For
                                    each=move || active_polls.get()
                                    key=|p| (p.id.clone(), p.options.iter().map(|o| o.votes).sum::<u32>())
                                    children=move |p| {
                                        let pid = p.id.clone();
                                        let pid_for_votes = pid.clone();
                                        view! {
                                            <div class="poll-card poll-item">
                                                <div class="poll-card-header">
                                                    <h4>{p.question.clone()}</h4>
                                                    <Show when=move || is_host.get()>
                                                        <button
                                                            class="btn btn-sm btn-danger"
                                                            on:click={
                                                                let pid_inner = pid.clone();
                                                                move |_| on_close_poll.run(pid_inner.clone())
                                                            }
                                                        >
                                                            "Close Poll"
                                                        </button>
                                                    </Show>
                                                </div>
                                                <ul class="poll-options-list">
                                                    <For
                                                        each={
                                                            let opts = p.options.clone();
                                                            let total_votes: u32 = opts.iter().map(|o| o.votes).sum();
                                                            move || opts.clone().into_iter().map(move |o| (o, total_votes)).collect::<Vec<_>>()
                                                        }
                                                        key=|tuple| tuple.0.id
                                                        children={
                                                            let pid_inner_cap = pid_for_votes.clone();
                                                            move |(opt, total_votes)| {
                                                                let pid_inner2 = pid_inner_cap.clone();
                                                                let percent = if total_votes > 0 {
                                                                    (opt.votes as f64 / total_votes as f64) * 100.0
                                                                } else {
                                                                    0.0
                                                                };

                                                                view! {
                                                                    <li class="poll-option-item">
                                                                        <div class="poll-option-content">
                                                                            <span class="poll-option-text">{opt.text}</span>
                                                                            <div class="poll-option-actions">
                                                                                <span class="poll-votes-badge">
                                                                                    {opt.votes} " votes (" {format!("{:.0}", percent)} "%)"
                                                                                </span>
                                                                                <button
                                                                                    class="btn btn-sm btn-primary"
                                                                                    on:click={
                                                                                        let pid_inner3 = pid_inner2.clone();
                                                                                        move |_| on_vote.run((pid_inner3.clone(), opt.id))
                                                                                    }
                                                                                >
                                                                                    "Vote"
                                                                                </button>
                                                                            </div>
                                                                        </div>
                                                                        <div
                                                                            class="poll-bar"
                                                                            style=format!("width: {}%;", percent)
                                                                        ></div>
                                                                    </li>
                                                                }
                                                            }
                                                        }
                                                    />
                                                </ul>
                                            </div>
                                        }
                                    }
                                />
                                <Show when=move || active_polls.get().is_empty()>
                                    <div class="empty-state">
                                        <p class="text-muted">"No active polls at the moment."</p>
                                    </div>
                                </Show>
                            </div>
                        </Show>

                        <Show when=move || active_tab.get() == "history">
                            <div class="polls-list">
                                <For
                                    each=move || history_polls.get()
                                    key=|p| (p.id.clone(), p.options.iter().map(|o| o.votes).sum::<u32>())
                                    children=move |p| {
                                        view! {
                                            <div class="poll-card poll-item closed">
                                                <div class="poll-card-header">
                                                    <h4>{p.question.clone()}</h4>
                                                    <span class="badge badge-closed">"CLOSED"</span>
                                                </div>
                                                <ul class="poll-options-list">
                                                    <For
                                                        each={
                                                            let opts = p.options.clone();
                                                            let total_votes: u32 = opts.iter().map(|o| o.votes).sum();
                                                            move || opts.clone().into_iter().map(move |o| (o, total_votes)).collect::<Vec<_>>()
                                                        }
                                                        key=|tuple| tuple.0.id
                                                        children=move |(opt, total_votes)| {
                                                            let percent = if total_votes > 0 {
                                                                (opt.votes as f64 / total_votes as f64) * 100.0
                                                            } else {
                                                                0.0
                                                            };
                                                            view! {
                                                                <li class="poll-option-item">
                                                                    <div class="poll-option-content">
                                                                        <span class="poll-option-text">{opt.text}</span>
                                                                        <span class="poll-votes-badge">
                                                                            {opt.votes} " votes (" {format!("{:.0}", percent)} "%)"
                                                                        </span>
                                                                    </div>
                                                                    <div
                                                                        class="poll-bar"
                                                                        style=format!("width: {}%;", percent)
                                                                    ></div>
                                                                </li>
                                                            }
                                                        }
                                                    />
                                                </ul>
                                            </div>
                                        }
                                    }
                                />
                                <Show when=move || history_polls.get().is_empty()>
                                    <div class="empty-state">
                                        <p class="text-muted">"No poll history available."</p>
                                    </div>
                                </Show>
                            </div>
                        </Show>

                        <Show when=move || active_tab.get() == "create">
                            <div class="poll-create-form">
                                <div class="form-group">
                                    <label class="form-label">"Poll Question"</label>
                                    <input
                                        type="text"
                                        id="poll-question"
                                        class="form-control"
                                        prop:value=question
                                        on:input=move |ev| set_question.set(event_target_value(&ev))
                                        placeholder="e.g. What is your favorite color?"
                                    />
                                </div>
                                <div class="form-group">
                                    <label class="form-label">"Option 1"</label>
                                    <input
                                        type="text"
                                        id="poll-option-1"
                                        class="form-control"
                                        prop:value=option1
                                        on:input=move |ev| set_option1.set(event_target_value(&ev))
                                        placeholder="Option 1"
                                    />
                                </div>
                                <div class="form-group">
                                    <label class="form-label">"Option 2"</label>
                                    <input
                                        type="text"
                                        id="poll-option-2"
                                        class="form-control"
                                        prop:value=option2
                                        on:input=move |ev| set_option2.set(event_target_value(&ev))
                                        placeholder="Option 2"
                                    />
                                </div>
                                <button
                                    id="create-poll-submit-btn"
                                    class="btn btn-success btn-full"
                                    on:click=create
                                >
                                    "Create Poll"
                                </button>
                            </div>
                        </Show>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use shared::Poll;
    use std::collections::HashSet;

    #[test]
    fn test_poll_filtering_logic() {
        let p1 = Poll {
            id: "1".to_string(),
            question: "Q1".to_string(),
            options: vec![],
            voters: HashSet::new(),
            is_closed: false,
            created_seq: 0,
        };
        let p2 = Poll {
            id: "2".to_string(),
            question: "Q2".to_string(),
            options: vec![],
            voters: HashSet::new(),
            is_closed: true,
            created_seq: 0,
        };

        let polls = [p1.clone(), p2.clone()];

        let active: Vec<_> = polls.iter().filter(|p| !p.is_closed).collect();
        let history: Vec<_> = polls.iter().filter(|p| p.is_closed).collect();

        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "1");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, "2");
    }
}
