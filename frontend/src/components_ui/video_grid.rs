use crate::components_ui::audio_level_indicator::AudioLevelIndicator;
use crate::components_ui::context_menu::VideoContextMenu;
use leptos::html;
use leptos::prelude::*;

use crate::cleanup::on_cleanup_local;
use shared::Participant;
use std::collections::{HashMap, HashSet};
use wasm_bindgen::JsCast;
use web_sys::MediaStream;

#[derive(Clone, PartialEq)]
enum GridItem {
    User(Participant),
    RemoteScreen(Participant),
    SharedVideo(String), // URL
}

impl GridItem {
    // Helper for key generation for DOM elements.
    // It should strictly represent identity, not mutable properties,
    // to prevent component teardown and flickering on state changes.
    #[allow(dead_code)]
    fn unique_key(&self) -> String {
        match self {
            GridItem::User(p) => p.id.clone(),
            GridItem::RemoteScreen(p) => format!("{}_screen", p.id),
            GridItem::SharedVideo(url) => format!("shared_video_{}", url),
        }
    }

    fn is_screen(&self) -> bool {
        matches!(self, GridItem::RemoteScreen(_))
    }

    #[allow(dead_code)]
    fn is_shared_video(&self) -> bool {
        matches!(self, GridItem::SharedVideo(_))
    }

    fn participant(&self) -> Option<&Participant> {
        match self {
            GridItem::User(p) => Some(p),
            GridItem::RemoteScreen(p) => Some(p),
            GridItem::SharedVideo(_) => None,
        }
    }
}

#[component]
pub fn VideoGrid(
    participants: ReadSignal<Vec<Participant>>,
    local_stream: ReadSignal<Option<MediaStream>>,
    local_screen_stream: ReadSignal<Option<MediaStream>>,
    my_audio_level: Signal<f64>,
    my_id: ReadSignal<Option<String>>,
    shared_video_url: ReadSignal<Option<String>>,
    speaking_peers: ReadSignal<HashSet<String>>,
    dominant_speaker: ReadSignal<Option<String>>,
    remote_streams: ReadSignal<HashMap<String, Vec<MediaStream>>>,
    layout: ReadSignal<String>,
    on_set_layout: Callback<String>,
    #[prop(optional)] pinned_participant: Option<ReadSignal<Option<String>>>,
    #[prop(optional)] is_audio_only: Option<Signal<bool>>,
    #[prop(optional)] is_flipped: Option<Signal<bool>>,
    #[prop(optional)] on_pin_participant: Option<Callback<Option<String>>>,
    #[prop(optional)] on_kick_participant: Option<Callback<String>>,
    #[prop(optional)] participant_volumes: Option<ReadSignal<HashMap<String, f64>>>,
    #[prop(optional)] on_set_voltage: Option<Callback<(String, f64)>>,
    #[prop(optional)] is_host: Option<Signal<bool>>,
) -> impl IntoView {
    let video_ref = NodeRef::<html::Video>::new();
    let screen_ref = NodeRef::<html::Video>::new();

    Effect::new(move |_| {
        if let Some(stream) = local_stream.get()
            && let Some(video_el) = video_ref.get()
        {
            video_el.set_src_object(Some(&stream));
            crate::utils::play_quietly(&video_el);
        }
    });

    Effect::new(move |_| {
        if let Some(stream) = local_screen_stream.get()
            && let Some(video_el) = screen_ref.get()
        {
            video_el.set_src_object(Some(&stream));
            crate::utils::play_quietly(&video_el);
        }
    });

    // Context menu state (opened on right-click of a remote tile)
    let (menu_open, set_menu_open) = signal(false);
    let (menu_x, set_menu_x) = signal(0i32);
    let (menu_y, set_menu_y) = signal(0i32);
    let (menu_target, set_menu_target) = signal(Option::<String>::None);

    let on_kick_sv = on_kick_participant.map(|cb| StoredValue::new(cb));
    let on_pin_sv = on_pin_participant.map(|cb| StoredValue::new(cb));
    let on_volume_sv = on_set_voltage.map(|cb| StoredValue::new(cb));

    // Prepare grid items: remote users + remote screens + shared video
    let grid_items = Memo::new(move |_| {
        let mut items = Vec::new();
        if let Some(url) = shared_video_url.get() {
            items.push(GridItem::SharedVideo(url));
        }

        let is_spotlight = layout.get() == "spotlight";
        let dominant = dominant_speaker.get();
        let my_id_val = my_id.get();
        let pinned = pinned_participant.and_then(|s| s.get());

        let list = participants.get();

        if is_spotlight {
            // Find spotlight participant: pinned takes priority over dominant speaker
            let spotlight_id = pinned.or(dominant).or_else(|| {
                list.iter()
                    .find(|p| Some(p.id.clone()) != my_id_val)
                    .map(|p| p.id.clone())
            });

            // Push the spotlighted participant first (rendered as the main tile),
            // then push the remaining remote participants so they appear as
            // thumbnails (filmstrip).
            if let Some(sid) = &spotlight_id
                && let Some(p) = list.iter().find(|p| &p.id == sid)
                && Some(p.id.clone()) != my_id_val
            {
                items.push(GridItem::User(p.clone()));
                if p.is_sharing_screen {
                    items.push(GridItem::RemoteScreen(p.clone()));
                }
            }

            for p in &list {
                if my_id_val.as_ref() == Some(&p.id) {
                    continue;
                }
                if spotlight_id.as_ref() == Some(&p.id) {
                    continue;
                }
                items.push(GridItem::User(p.clone()));
                if p.is_sharing_screen {
                    items.push(GridItem::RemoteScreen(p.clone()));
                }
            }
        } else {
            for p in list {
                if my_id_val != Some(p.id.clone()) {
                    items.push(GridItem::User(p.clone()));
                    if p.is_sharing_screen {
                        items.push(GridItem::RemoteScreen(p.clone()));
                    }
                }
            }
        }
        items
    });

    let (layout_open, set_layout_open) = signal(false);
    let layout_icon_label = |l: &str| {
        if l == "spotlight" {
            "Speaker view"
        } else {
            "Tile view"
        }
    };

    view! {
        <div class="video-grid-container">
            <div class="layout-menu-wrapper">
                <button
                    class="btn btn-outline layout-menu-btn"
                    on:click=move |_| set_layout_open.update(|v| *v = !*v)
                    title="Video layout"
                >
                    {move || format!("▦ {}", layout_icon_label(&layout.get()))}
                </button>
                <Show when=move || layout_open.get()>
                    <div class="layout-menu">
                        <button
                            class=move || format!("layout-option {}", if layout.get() == "grid" { "active" } else { "" })
                            on:click=move |_| { on_set_layout.run("grid".to_string()); set_layout_open.set(false); }
                        >"Tile view"</button>
                        <button
                            class=move || format!("layout-option {}", if layout.get() == "spotlight" { "active" } else { "" })
                            on:click=move |_| { on_set_layout.run("spotlight".to_string()); set_layout_open.set(false); }
                        >"Speaker view"</button>
                    </div>
                </Show>
            </div>

            // ---- Local screen share tile (feature area in spotlight) ----
            <Show when=move || local_screen_stream.get().is_some()>
                <div class="video-card local-screen" class:featured=move || layout.get() == "spotlight">
                    <video
                        node_ref=screen_ref
                        autoplay
                        playsinline
                        muted
                    />
                    <button
                        on:click=move |_| {
                            if let Some(video) = screen_ref.get() {
                                let js_video: &wasm_bindgen::JsValue = video.as_ref();
                                let prop = wasm_bindgen::JsValue::from_str("requestPictureInPicture");
                                if let Ok(func) = js_sys::Reflect::get(js_video, &prop)
                                    && let Some(func) = func.dyn_ref::<js_sys::Function>() {
                                        let promise = func.call0(js_video);
                                        let _ = promise;
                                    }
                            }
                        }
                        class="pip-btn"
                        title="Picture-in-Picture"
                    >
                        "PiP"
                    </button>
                    <div class="name-tag">"My Screen"</div>
                </div>
            </Show>

            // ---- All remote tiles, structured spotlight vs tiling ----
            {move || {
                let items = grid_items.get();
                if layout.get() == "spotlight" {
                    let (featured, filmstrip) = if items.is_empty() {
                        (Vec::new(), Vec::new())
                    } else {
                        (items[..1].to_vec(), items[1..].to_vec())
                    };
                    view! {
                        <div class="video-grid spotlight">
                        <div class="spotlight-main">
                            {featured.into_iter().map(|item| {
                                render_remote_item(
                                    item, participants, my_id, is_audio_only, is_flipped,
                                    speaking_peers, remote_streams, layout, pinned_participant,
                                    true, set_menu_open, set_menu_x, set_menu_y, set_menu_target,
                                )
                            }).collect_view()}
                        </div>
                        <div class="filmstrip">
                            { render_local_user_tile(local_stream, my_id, participants, my_audio_level, speaking_peers, is_flipped, video_ref) }
                            {filmstrip.into_iter().map(|item| {
                                render_remote_item(
                                    item, participants, my_id, is_audio_only, is_flipped,
                                    speaking_peers, remote_streams, layout, pinned_participant,
                                    false, set_menu_open, set_menu_x, set_menu_y, set_menu_target,
                                )
                            }).collect_view()}
                        </div>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="video-grid grid">
                            { render_local_user_tile(local_stream, my_id, participants, my_audio_level, speaking_peers, is_flipped, video_ref) }
                            {items.into_iter().map(|item| {
                                render_remote_item(
                                    item, participants, my_id, is_audio_only, is_flipped,
                                    speaking_peers, remote_streams, layout, pinned_participant,
                                    false, set_menu_open, set_menu_x, set_menu_y, set_menu_target,
                                )
                            }).collect_view()}
                        </div>
                    }.into_any()
                }
            }}

            // ---- Video context menu ----
            <VideoContextMenu
                open=menu_open
                x=menu_x
                y=menu_y
                is_host=is_host.unwrap_or(Signal::derive(|| false))
                is_pinned=Signal::derive(move || menu_target.with(|t| t.as_ref().map(|t| pinned_participant.and_then(|s| s.get()) == Some(t.clone())).unwrap_or(false)))
                volume=Signal::derive(move || {
                    let target = menu_target.get();
                    let vols = participant_volumes.map(|s| s.get());
                    target.and_then(|t| vols.as_ref().and_then(|m| m.get(&t).copied())).unwrap_or(1.0)
                })
                on_pin=Callback::new(move |_| {
                    if let Some(t) = menu_target.get_untracked()
                        && let Some(cb) = on_pin_sv {
                            let pinned_now = pinned_participant.and_then(|s| s.get());
                            cb.get_value().run(if pinned_now == Some(t.clone()) { None } else { Some(t) });
                        }
                })
                on_kick=Callback::new(move |_| {
                    if let Some(t) = menu_target.get_untracked()
                        && let Some(cb) = on_kick_sv {
                            cb.get_value().run(t);
                        }
                })
                on_volume=Callback::new(move |v: f64| {
                    if let Some(t) = menu_target.get_untracked()
                        && let Some(cb) = on_volume_sv {
                            cb.get_value().run((t, v));
                        }
                })
                on_close=Callback::new(move |_| set_menu_open.set(false))
            />
        </div>
    }
}

/// Renders the local-user tile (self preview).
fn render_local_user_tile(
    local_stream: ReadSignal<Option<MediaStream>>,
    my_id: ReadSignal<Option<String>>,
    participants: ReadSignal<Vec<Participant>>,
    my_audio_level: Signal<f64>,
    speaking_peers: ReadSignal<HashSet<String>>,
    is_flipped: Option<Signal<bool>>,
    video_ref: NodeRef<html::Video>,
) -> AnyView {
    view! {
        <div class="video-card local-video">
            <Show
                when=move || {
                    local_stream.get()
                        .map(|s| s.get_video_tracks().length() > 0)
                        .unwrap_or(false)
                }
                fallback=move || {
                    let my_id_val = my_id.get();
                    let my_id_val_c = my_id_val.clone();
                    let avatar_url = Signal::derive(move || {
                        my_id_val.clone().and_then(|id| {
                            participants.with(|ps| {
                                ps.iter().find(|p| p.id == id).and_then(|p| p.avatar_url.clone())
                            })
                        })
                    });
                    let initial = Signal::derive(move || {
                        my_id_val_c.clone().and_then(|id| {
                            participants.with(|ps| {
                                ps.iter().find(|p| p.id == id).map(|p| p.name.chars().next().unwrap_or('?').to_uppercase().to_string())
                            })
                        }).unwrap_or_else(|| "Me".to_string())
                    });
                    let (avatar_failed, set_avatar_failed) = signal(false);
                    Effect::new(move |_| {
                        let _ = avatar_url.get();
                        set_avatar_failed.set(false);
                    });
                    view! {
                        <div class="video-placeholder">
                            <span class="sr-only">"Camera Off"</span>
                            <div class="avatar-container">
                                <Show when=move || avatar_url.get().is_some() && !avatar_failed.get() fallback=move || view! {
                                    <div class="avatar">{initial.get()}</div>
                                }>
                                    <img
                                        src=move || avatar_url.get().unwrap_or_default()
                                        on:error=move |_| set_avatar_failed.set(true)
                                        class="avatar-img"
                                        alt="Avatar"
                                    />
                                </Show>
                            </div>
                        </div>
                    }
                }
            >
                <video
                    node_ref=video_ref
                    autoplay
                    playsinline
                    muted
                    class=move || if is_flipped.map(|s| s.get()).unwrap_or(true) { "flipped" } else { "" }
                />
                <button
                    on:click=move |_| {
                        if let Some(video) = video_ref.get() {
                            let js_video: &wasm_bindgen::JsValue = video.as_ref();
                            let prop = wasm_bindgen::JsValue::from_str("requestPictureInPicture");
                            if let Ok(func) = js_sys::Reflect::get(js_video, &prop)
                                && let Some(func) = func.dyn_ref::<js_sys::Function>() {
                                    let promise = func.call0(js_video);
                                    let _ = promise;
                                }
                        }
                    }
                    class="pip-btn"
                    title="Picture-in-Picture"
                >
                    "PiP"
                </button>
            </Show>
            <Show when=move || speaking_peers.get().contains(&my_id.get().unwrap_or_default())>
                <div class="speaking-ring"></div>
            </Show>
            <div class="name-tag">"Me"</div>
            <div class="status-icons">
                <Show when=move || {
                    my_id.get().map(|id| {
                        participants.with(|ps| {
                            ps.iter().find(|p| p.id == id).map(|p| p.e2ee_enabled).unwrap_or(false)
                        })
                    }).unwrap_or(false)
                }>
                    <span class="e2ee-lock" title="End-to-End Encrypted">"🔒"</span>
                </Show>
                <Show when=move || {
                    my_id.get().map(|id| {
                        participants.with(|ps| {
                            ps.iter().find(|p| p.id == id).map(|p| p.is_hand_raised).unwrap_or(false)
                        })
                    }).unwrap_or(false)
                }>
                    <span title="Hand Raised">"✋"</span>
                </Show>
                <Show when=move || { my_audio_level.get() > 0.0 }>
                    <AudioLevelIndicator audio_level=my_audio_level />
                </Show>
            </div>
        </div>
    }.into_any()
}

/// Renders one remote grid item (remote user / remote screen / shared video).
#[allow(clippy::too_many_arguments)]
fn render_remote_item(
    item: GridItem,
    participants: ReadSignal<Vec<Participant>>,
    _my_id: ReadSignal<Option<String>>,
    is_audio_only: Option<Signal<bool>>,
    _is_flipped: Option<Signal<bool>>,
    speaking_peers: ReadSignal<HashSet<String>>,
    remote_streams: ReadSignal<HashMap<String, Vec<MediaStream>>>,
    _layout: ReadSignal<String>,
    pinned_participant: Option<ReadSignal<Option<String>>>,
    featured: bool,
    set_menu_open: WriteSignal<bool>,
    set_menu_x: WriteSignal<i32>,
    set_menu_y: WriteSignal<i32>,
    set_menu_target: WriteSignal<Option<String>>,
) -> AnyView {
    match item {
        GridItem::SharedVideo(url) => {
            // Same parser the dialog validates with, so a URL that was accepted
            // can never render as an empty embed.
            let embed_url = match crate::components_ui::shared_video_dialog::youtube_video_id(&url)
            {
                Some(id) => format!("https://www.youtube.com/embed/{}?autoplay=1", id),
                None => String::new(),
            };
            view! {
                <div class="video-card shared-video">
                    <iframe
                        width="100%"
                        height="100%"
                        src=embed_url
                        style="border: 0;"
                        allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
                        allowfullscreen
                    ></iframe>
                    <div class="name-tag">"Shared Video"</div>
                </div>
            }.into_any()
        }
        _ => {
            let p = item.participant().unwrap().clone();
            let is_screen = item.is_screen();
            let id = p.id.clone();
            let id_for_stream = id.clone();
            let id_for_ctx = id.clone();
            let id_for_pinicon = id.clone();

            let id_s = id.clone();
            let p_name = Signal::derive(move || {
                participants.with(|ps| {
                    ps.iter()
                        .find(|pp| pp.id == id_s)
                        .map(|pp| {
                            if is_screen {
                                format!("{}'s Screen", pp.name)
                            } else {
                                pp.name.clone()
                            }
                        })
                        .unwrap_or_else(|| "Unknown".to_string())
                })
            });

            let id_hand = id.clone();
            let is_hand_raised = Signal::derive(move || {
                participants.with(|ps| {
                    ps.iter()
                        .find(|pp| pp.id == id_hand)
                        .map(|pp| pp.is_hand_raised)
                        .unwrap_or(false)
                })
            });

            let id_e2ee = id.clone();
            let is_p_e2ee = Signal::derive(move || {
                participants.with(|ps| {
                    ps.iter()
                        .find(|pp| pp.id == id_e2ee)
                        .map(|pp| pp.e2ee_enabled)
                        .unwrap_or(false)
                })
            });

            let id_init = id.clone();
            let initial_char = Signal::derive(move || {
                participants.with(|ps| {
                    ps.iter()
                        .find(|pp| pp.id == id_init)
                        .and_then(|pp| pp.name.chars().next())
                        .unwrap_or('?')
                        .to_uppercase()
                        .to_string()
                })
            });

            let id_av = id.clone();
            let avatar_url_sig = Signal::derive(move || {
                participants.with(|ps| {
                    ps.iter()
                        .find(|pp| pp.id == id_av)
                        .and_then(|pp| pp.avatar_url.clone())
                })
            });
            let (avatar_failed, set_avatar_failed) = signal(false);
            Effect::new(move |_| {
                let _ = avatar_url_sig.get();
                set_avatar_failed.set(false);
            });

            let id_speak = id.clone();
            let is_speaking = Memo::new(move |_| speaking_peers.get().contains(&id_speak));
            let (audio_level_sig, set_audio_level_sig) = signal(0.0f64);
            Effect::new(move |_| {
                if is_speaking.get() {
                    let window = web_sys::window().unwrap();
                    let cb = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                        let random_val = js_sys::Math::random() * 0.8;
                        set_audio_level_sig.set(random_val);
                    })
                        as Box<dyn FnMut()>);
                    let interval_id = window
                        .set_interval_with_callback_and_timeout_and_arguments_0(
                            cb.as_ref().unchecked_ref(),
                            200,
                        )
                        .unwrap();
                    on_cleanup_local(move || {
                        let window = web_sys::window().unwrap();
                        window.clear_interval_with_handle(interval_id);
                        drop(cb);
                    });
                } else {
                    set_audio_level_sig.set(0.0);
                }
            });

            let remote_video_ref = NodeRef::<html::Video>::new();
            let stream_signal = Signal::derive(move || {
                remote_streams.with(|map| {
                    if let Some(streams) = map.get(&id_for_stream) {
                        if is_screen {
                            // Prefer displaySurface-tagged stream; else fall back to last.
                            let screen_stream = streams.iter().find(|s| {
                                let tracks = s.get_video_tracks();
                                for i in 0..tracks.length() {
                                    let track_val = tracks.get(i);
                                    if let Ok(track) =
                                        track_val.dyn_into::<web_sys::MediaStreamTrack>()
                                    {
                                        let settings = track.get_settings();
                                        if let Ok(val) = js_sys::Reflect::get(
                                            &settings,
                                            &"displaySurface".into(),
                                        ) && !val.is_undefined()
                                        {
                                            return true;
                                        }
                                    }
                                }
                                false
                            });
                            screen_stream.or_else(|| streams.last()).cloned()
                        } else {
                            streams.first().cloned()
                        }
                    } else {
                        None
                    }
                })
            });
            Effect::new(move |_| {
                if let Some(stream) = stream_signal.get()
                    && let Some(video_el) = remote_video_ref.get()
                {
                    video_el.set_src_object(Some(&stream));
                    crate::utils::play_quietly(&video_el);
                }
            });

            let css_featured = if featured { "featured spotlighted" } else { "" };
            let css_screen = if is_screen { "screen-share" } else { "" };
            let is_speaking_class =
                Signal::derive(move || if is_speaking.get() { "speaking" } else { "" });

            // Right-click handler opens the context menu with position clamping
            let menu_target_open = move |ev: web_sys::MouseEvent| {
                ev.prevent_default();
                if is_screen {
                    return;
                }
                let mut x = ev.client_x();
                let mut y = ev.client_y();
                if let Some(window) = web_sys::window() {
                    let win_w = window
                        .inner_width()
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(1024.0) as i32;
                    let win_h = window
                        .inner_height()
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(768.0) as i32;
                    if x + 180 > win_w {
                        x = (win_w - 190).max(10);
                    }
                    if y + 200 > win_h {
                        y = (win_h - 210).max(10);
                    }
                }
                set_menu_x.set(x);
                set_menu_y.set(y);
                set_menu_target.set(Some(id_for_ctx.clone()));
                set_menu_open.set(true);
            };

            view! {
                <div
                    class=move || format!("video-card {} {} {}", css_featured, is_speaking_class.get(), css_screen)
                    on:contextmenu=menu_target_open
                >
                    <Show when=move || stream_signal.get().is_some() && !is_audio_only.map(|s| s.get()).unwrap_or(false) fallback=move || {
                        if is_screen {
                            view! {
                                <div class="screen-placeholder">"Waiting for screen..."</div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="avatar-container">
                                    <Show when=move || avatar_url_sig.get().is_some() && !avatar_failed.get() fallback=move || view! {
                                        <div class="avatar">{initial_char.get()}</div>
                                    }>
                                        <img
                                            src=move || avatar_url_sig.get().unwrap_or_default()
                                            on:error=move |_| set_avatar_failed.set(true)
                                            class="avatar-img"
                                            alt="Avatar"
                                        />
                                    </Show>
                                </div>
                            }.into_any()
                        }
                    }>
                        <video
                            node_ref=remote_video_ref
                            autoplay
                            playsinline
                            class=move || if is_screen { "screen-share" } else { "" }
                        />
                    </Show>
                    <button
                        on:click=move |_| {
                            if let Some(video) = remote_video_ref.get() {
                                let js_video: &wasm_bindgen::JsValue = video.as_ref();
                                let prop = wasm_bindgen::JsValue::from_str("requestPictureInPicture");
                                if let Ok(func) = js_sys::Reflect::get(js_video, &prop)
                                    && let Some(func) = func.dyn_ref::<js_sys::Function>() {
                                        let promise = func.call0(js_video);
                                        let _ = promise;
                                    }
                            }
                        }
                        class="pip-btn"
                        title="Picture-in-Picture"
                    >
                        "PiP"
                    </button>
                    <div class="name-tag">{move || p_name.get()}</div>
                    <div class="status-icons">
                        <Show when=move || is_p_e2ee.get() && !is_screen>
                            <span class="e2ee-lock" title="End-to-End Encrypted">"🔒"</span>
                        </Show>
                        <Show when=move || is_hand_raised.get() && !is_screen>
                            <span title="Hand Raised">"✋"</span>
                        </Show>
                        <AudioLevelIndicator audio_level=audio_level_sig />
                        <Show when=move || {
                            let id = id_for_pinicon.clone();
                            pinned_participant.and_then(|s| s.get()) == Some(id)
                        }>
                            <span title="Pinned">"📍"</span>
                        </Show>
                    </div>
                </div>
            }.into_any()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::Participant;

    #[test]
    fn test_grid_item_key() {
        let p = Participant {
            id: "user1".to_string(),
            name: "Alice".to_string(),
            is_hand_raised: false,
            is_sharing_screen: false,
            is_muted: false,
            is_camera_muted: false,
            speaking_time: 0,
            presence: shared::PresenceStatus::Connected,
            is_visitor: false,
            e2ee_enabled: false,
            hand_raised_at: None,
            avatar_url: None,
        };

        let item_user = GridItem::User(p.clone());
        assert_eq!(item_user.unique_key(), "user1");
        assert!(!item_user.is_screen());

        let item_screen = GridItem::RemoteScreen(p.clone());
        assert_eq!(item_screen.unique_key(), "user1_screen");
        assert!(item_screen.is_screen());

        let item_video = GridItem::SharedVideo("http://test".to_string());
        assert_eq!(item_video.unique_key(), "shared_video_http://test");
        assert!(item_video.is_shared_video());
    }
}
