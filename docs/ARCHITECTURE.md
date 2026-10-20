# Architecture

Juncto is a single Cargo workspace with three crates. The client is compiled to
WebAssembly; the server is a normal native binary. They communicate over one WebSocket
endpoint using message types defined in a third crate that both depend on.

```
┌──────────────────────────┐        WebSocket (/ws/chat)        ┌──────────────────────────┐
│        frontend          │  ────────────────────────────────► │         backend          │
│  Leptos CSR → wasm32     │  ◄──────────────────────────────── │  Axum + tokio + broadcast │
│  frontend/pkg/*.wasm     │      shared::ClientMessage         │  in-process room state    │
└──────────────────────────┘      shared::ServerMessage         └──────────────────────────┘
             │                                                              │
             └──────────────────────┬───────────────────────────────────────┘
                                    ▼
                        ┌──────────────────────────┐
                        │          shared          │
                        │  serde types for both    │
                        │  sides of the connection │
                        └──────────────────────────┘
```

## Crates

### `shared` — the protocol

`shared/src/lib.rs` is the contract. It defines `Participant`, `ChatMessage`, `Poll`,
`RoomConfig`, draw actions, presence and the two top-level enums:

* `ClientMessage` — everything the client can send (join, chat, poll votes, draw actions,
  moderation toggles, breakout commands, remote-control signalling, …).
* `ServerMessage` — everything the server can push (welcome, participant updates, chat
  history, poll results, knocking notifications, errors, …).

Both enums derive `serde::{Serialize, Deserialize}`. Because the same Rust type is
compiled into the WASM client and the native server, adding a variant to one side and not
the other is a compile error rather than a silent protocol drift.

`RoomConfig` deliberately keeps `access_password` out of the wire format:

```rust
#[serde(skip_serializing, skip_deserializing)]
pub access_password: Option<String>,
```

The room password therefore never leaves the process. Clients only learn whether a lock is
password-protected through the separate, safe-to-send `has_password` flag.

### `backend` — the server

`backend/src/main.rs` builds the Axum router and holds the application state.

| Route | Method | Handler | Purpose |
|---|---|---|---|
| `/` and everything unmatched | GET | `ServeDir(frontend/pkg)` + `index.html` fallback | Serves the WASM client |
| `/static/*` | GET | `ServeDir(backend/static)` | Stylesheet and other static assets |
| `/api/rooms` | POST | `handlers::room::create_room` | Creates or resets a room's configuration |
| `/api/feedback` | POST | `handlers::feedback::submit_feedback` | Records feedback submissions |
| `/health` | GET | `api::health_check` | Liveness probe used by the test suite |
| `/ws/chat` | GET (upgrade) | `handlers::ws::chat_handler` | The single multiplexed WebSocket |

`backend/src/api.rs` is a thin re-export surface that wires the feature handlers into the
router and owns the `health_check` endpoint.

Static directories are resolved from `env!("CARGO_MANIFEST_DIR")`:

```rust
let frontend_pkg = format!("{}/../frontend/pkg", env!("CARGO_MANIFEST_DIR"));
let static_dir = format!("{}/static", env!("CARGO_MANIFEST_DIR"));
```

This is baked in at compile time, so the binary serves the client correctly whether you
launch it from the repository root, from `tests/e2e`, or anywhere else. The Playwright
suite relies on this, since it starts the server from a different working directory.

#### State and fan-out

Room state lives in memory behind mutexes, one set per room. The most important pieces:

* `participants` — id → `Participant`, the source of truth for the roster.
* `room_config` — lock/password/lobby/max-participants/host id.
* `chat_history` — replayed to late joiners so they see the conversation so far.
* `polls`, `whiteboard`, `breakout` — per-feature stores.
* `knocking` — pending lobby requests, each paired with a oneshot approval channel.
* A `tokio::sync::broadcast::Sender<ServerMessage>` used as the fan-out bus. Each connected
  socket subscribes; the handler also keeps a direct channel for messages addressed to
  that one client (`Welcome`, `Error`, …).

Feature endpoints and message handling are split by concern under `backend/src/handlers/`:
`ws.rs` holds the WebSocket loop and dispatch, `chat.rs`, `polls.rs`, `whiteboard.rs`,
`breakout.rs`, `moderation.rs`, `room.rs`, `feedback.rs`, `calendar.rs`, `salesforce.rs`,
`dropbox.rs` and `remote_control.rs` hold the per-feature logic.

The handler treats each variant explicitly, so a single connection carries chat, polls,
whiteboard strokes, breakout transitions, moderation and remote-control signalling
together.

#### Authorization decisions

Notable server-side rules:

* **Host election.** The first non-visitor to join a room that has no live host becomes
  host. If the recorded host disconnects, `host_id` is cleared so the next joiner can take
  over — a stale host id can never block a room.
* **Lobby.** When the lobby is enabled *and* a host is present, a joining client is given a
  `Knocking` message and a 120-second window for the host to approve or reject. If the
  window expires, the knocking entry is removed and a `KnockingParticipantLeft` is
  broadcast. Without a host present the lobby does not engage, so a room can never be
  deadlocked by an empty waiting room.
* **Lock.** A locked room with a password accepts only an exact match; a locked room
  without a password rejects everyone. Both cases produce an explicit `Error` message.
* **Avatar sanitisation.** An avatar URL is only accepted if it is `https://` and at most
  2048 characters; an invalid value is dropped rather than rejecting the join.
* **Capacity.** `max_participants` is enforced while the participants lock is held, so
  concurrent joins cannot overshoot the limit.
* **Room reset.** `POST /api/rooms` replaces the global room configuration and clears all
  live state, so it requires the bearer token from `ROOM_ADMIN_TOKEN`. The endpoint is
  disabled while that variable is unset.
* **Bounded state.** Chat history, whiteboard strokes and polls are capped (oldest entries
  are evicted) and inbound WebSocket messages are size-limited, so a single client cannot
  exhaust server memory by sending indefinitely.

### `frontend` — the client

A Leptos client-side-rendered application. There is no server-side rendering: `lib.rs`
mounts at the `index.html` root and takes over routing.

* `lib.rs` — mount point and router (`Home` and `Room`).
* `pages/` — `home.rs` (landing, recent meetings) and `room.rs` (the conference screen,
  which composes every dialog and panel).
* `state.rs` / `state_handlers.rs` — the reactive store. `state.rs` owns the WebSocket,
  signalling and the signals the UI reads; `state_handlers.rs` maps incoming
  `ServerMessage`s onto those signals.
* `webrtc.rs`, `media.rs`, `media_recorder.rs` — peer connections, device capture, screen
  share and local recording via `web-sys`.
* Feature modules at the crate root: `chat.rs`, `polls.rs`, `whiteboard.rs`,
  `reactions.rs`, `participants.rs`, `settings.rs`, `speaker_stats.rs`,
  `virtual_background.rs`, `shortcuts.rs`, `toolbox.rs`, `remote_control.rs`,
  `analytics.rs`, `face_landmarks.rs`, `connection_stats.rs`, `power_monitor.rs`,
  `deeplink.rs`, `storage.rs`, `i18n.rs`.
* `components_ui/` — the dialogs and widgets: prejoin, lobby, rejoin overlay, video grid,
  file sharing, invite, feedback, calendar, dial-in, Salesforce, Etherpad, embed, GIF,
  breakout, authentication, toast, context menu, always-on-top, audio-level and
  connection indicators, shared video.

Signals are `create_signal`/`ReadSignal` values derived from one another, so a change in
connection state or the participant list propagates to every dependent view. Side panels
(chat, participants, files) share one `active_panel` signal, which makes them mutually
exclusive — that keeps the video stage from being squeezed or overlapped. On viewports at
or below 768 px the panel is full-width, so it starts closed and the stage stays visible.

## Media path

Juncto uses a WebRTC mesh: every participant holds an `RTCPeerConnection` to every other
participant. Signalling is not a separate channel — SDP offers/answers and ICE candidates
travel as `ClientMessage` variants over the same `/ws/chat` socket, and the server relays
them to the addressed peer.

```
browser A ──offer──► backend ──relay──► browser B
browser A ◄──answer─ backend ◄──relay── browser B
browser A ◄─ICE────► backend ◄──ICE───► browser B
```

Consequences worth knowing:

* Media flows peer-to-peer; the server only relays signalling and room events.
* A full mesh means upload bandwidth grows with participant count. That suits small
  meetings; larger deployments would need an SFU, which is out of scope here.
* Because the transport is a plain WebSocket over HTTP, the deployment story stays simple.

## Static asset flow

```
build.sh
  ├─ cargo build --target wasm32-unknown-unknown --release   → target/…/frontend.wasm
  ├─ wasm-bindgen --target web --out-dir frontend/pkg        → frontend/pkg/frontend.{js,wasm}
  └─ cp frontend/index.html frontend/pkg/index.html
```

At runtime the server serves `frontend/pkg` at the root and falls back to `index.html`,
which gives the client-side router its deep-link behaviour: loading `/room/<name>`
directly returns the shell and Leptos routes on the client. The stylesheet lives in
`backend/static/styles.css` and is served under `/static`.

## Testing architecture

The suites in `tests/e2e/` all drive the real application — there are no API mocks. The
Playwright config boots the workspace with `./build.sh && cargo run --bin backend` and
waits for port 3000, then runs serially (`workers: 1`) because the backend keeps room state
in memory.

| Suite | Purpose |
|---|---|
| `migration*.spec.ts`, `conference_core.spec.ts`, `full_lifecycle.spec.ts` | Feature parity with the legacy client |
| `contrast-audit.spec.ts` | Measures resolved foreground/background colours in every dialog and asserts a WCAG contrast floor |
| `screenshot-gallery.spec.ts` | Captures every screen to `tests/screenshots/` for the README |

The contrast audit exists because a screenshot can look plausible while a dialog renders
near-white text on a near-white panel. It reads computed styles from the live DOM,
composites translucent backgrounds up the ancestor chain to get the true backdrop, and
fails the build when the resulting ratio drops below the threshold.

See [DEVELOPMENT.md](./DEVELOPMENT.md) for how to run these and
[FEATURES.md](./FEATURES.md) for the status of each feature.