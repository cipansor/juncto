use leptos::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum ToastType {
    Info,
    Error,
    Warning,
    Success,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub message: String,
    pub toast_type: ToastType,
    pub persistent: bool,
    pub priority: u8, // Higher is more important
}

#[derive(Clone, Copy)]
pub struct ToastContext {
    pub toasts: RwSignal<Vec<Toast>>,
    pub history: RwSignal<Vec<Toast>>,
    pub unread: RwSignal<u32>,
    counter: RwSignal<u64>,
}

impl ToastContext {
    pub fn add(&self, message: String, toast_type: ToastType) {
        self.add_advanced(message, toast_type, false, 0);
    }

    pub fn add_advanced(
        &self,
        message: String,
        toast_type: ToastType,
        persistent: bool,
        priority: u8,
    ) {
        let id = self.counter.get_untracked() + 1;
        self.counter.set(id);

        let toast = Toast {
            id,
            message: message.clone(),
            toast_type,
            persistent,
            priority,
        };

        self.toasts.update(|t| {
            t.push(toast.clone());
            // Sort by priority (descending)
            t.sort_by_key(|b| std::cmp::Reverse(b.priority));
        });

        self.history.update(|h| {
            h.push(toast);
        });
        self.unread.update(|v| *v += 1);

        #[cfg(target_arch = "wasm32")]
        if !persistent {
            let toasts = self.toasts;
            set_timeout(
                move || {
                    toasts.update(|t| t.retain(|item| item.id != id));
                },
                std::time::Duration::from_secs(if priority > 0 { 6 } else { 3 }),
            );
        }
    }

    pub fn remove(&self, id: u64) {
        self.toasts.update(|t| t.retain(|item| item.id != id));
    }

    pub fn mark_all_read(&self) {
        self.unread.set(0);
    }

    pub fn clear_history(&self) {
        self.history.update(|h| h.clear());
        self.unread.set(0);
    }
}

pub fn provide_toast_context() {
    provide_context(ToastContext {
        toasts: RwSignal::new(Vec::new()),
        history: RwSignal::new(Vec::new()),
        unread: RwSignal::new(0u32),
        counter: RwSignal::new(0),
    });
}

pub fn use_toast() -> ToastContext {
    use_context::<ToastContext>().expect("ToastContext must be provided")
}

#[component]
pub fn NotificationBell() -> impl IntoView {
    let ctx = use_toast();
    let (open, set_open) = signal(false);

    view! {
        <div style="position: absolute; right: 12px; top: 12px; z-index: 9000;">
            <button
                id="notif-bell-btn"
                title="Notifications"
                on:click=move |_| {
                    let opening = !open.get();
                    set_open.set(opening);
                    if opening {
                        ctx.mark_all_read();
                    }
                }
                style="background: rgba(0,0,0,0.4); color: white; border: none; padding: 6px 10px; border-radius: 4px; cursor: pointer; font-size: 16px;"
            >
                "🔔"
                <Show when=move || { ctx.unread.get() > 0 }>
                    <span class="notif-badge">
                        {move || ctx.unread.get()}
                    </span>
                </Show>
            </button>
            <Show when=move || open.get()>
                <div id="notif-panel" style="position: absolute; right: 0; top: 36px; width: 280px; max-height: 300px; overflow-y: auto; background: #1e1e1e; color: #eee; border-radius: 8px; box-shadow: 0 2px 8px rgba(0,0,0,0.4); padding: 8px;">
                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px;">
                        <h4 style="margin: 0;">"Notifications"</h4>
                        <button on:click=move |_| ctx.clear_history() style="background: none; border: none; color: #999; cursor: pointer; font-size: 12px;">"Clear"</button>
                    </div>
                    <Show
                        when=move || !ctx.history.get().is_empty()
                        fallback=move || view! { <div style="color: #999;">"No notifications"</div> }
                    >
                        <For
                            each=move || {
                                let mut items = ctx.history.get();
                                items.reverse();
                                items
                            }
                            key=|t| t.id
                            children=move |t| {
                                let cls = match t.toast_type {
                                    ToastType::Error => "notif-item notif-error",
                                    ToastType::Success => "notif-item notif-success",
                                    ToastType::Warning => "notif-item notif-warning",
                                    ToastType::Info => "notif-item notif-info",
                                };
                                view! { <div class=cls style="padding: 6px 8px; border-bottom: 1px solid #333; font-size: 13px;">{t.message}</div> }
                            }
                        />
                    </Show>
                </div>
            </Show>
        </div>
    }
}

#[component]
pub fn ToastContainer() -> impl IntoView {
    let ctx = use_toast();

    view! {
        <div class="toast-container" style="position: fixed; top: 20px; right: 20px; z-index: 10000; display: flex; flex-direction: column; gap: 10px;">
            <For
                each=move || ctx.toasts.get()
                key=|t| t.id
                children=move |t| {
                    let style = match t.toast_type {
                        ToastType::Info => "background: #007bff; color: white;",
                        ToastType::Error => "background: #dc3545; color: white;",
                        ToastType::Warning => "background: #ffc107; color: black;",
                        ToastType::Success => "background: #28a745; color: white;",
                    };
                    let id = t.id;
                    let class_name = match t.toast_type {
                        ToastType::Info => "toast toast-info",
                        ToastType::Error => "toast toast-error",
                        ToastType::Warning => "toast toast-warning",
                        ToastType::Success => "toast toast-success",
                    };
                    view! {
                        <div
                            class=class_name
                            style=format!("padding: 10px 20px; border-radius: 4px; box-shadow: 0 2px 4px rgba(0,0,0,0.2); min-width: 200px; animation: fadein 0.3s; cursor: pointer; {}", style)
                            on:click=move |_| ctx.remove(id)
                        >
                            {t.message}
                        </div>
                    }
                }
            />
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_types() {
        let t1 = ToastType::Info;
        let t2 = ToastType::Error;
        let t3 = ToastType::Success;
        let t4 = ToastType::Warning;
        assert_ne!(t1, t2);
        assert_ne!(t2, t3);
        assert_ne!(t3, t4);
    }

    #[test]
    fn test_toast_context_logic() {
        let owner = Owner::new();
        owner.set();
        let ctx = ToastContext {
            toasts: RwSignal::new(Vec::new()),
            history: RwSignal::new(Vec::new()),
            unread: RwSignal::new(0u32),
            counter: RwSignal::new(0),
        };

        ctx.add_advanced("Msg 1".to_string(), ToastType::Info, false, 0);
        ctx.add_advanced("Msg 2".to_string(), ToastType::Error, true, 10);

        let toasts = ctx.toasts.get();
        assert_eq!(toasts.len(), 2);
        // Should be sorted by priority
        assert_eq!(toasts[0].message, "Msg 2");
        assert_eq!(toasts[1].message, "Msg 1");
        assert!(toasts[0].persistent);
        assert!(!toasts[1].persistent);

        assert_eq!(ctx.history.get().len(), 2);
        assert_eq!(ctx.unread.get(), 2);
        ctx.mark_all_read();
        assert_eq!(ctx.unread.get(), 0);
    }
}
