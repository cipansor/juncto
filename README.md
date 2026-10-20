# <p align="center">Juncto</p>

<p align="center">
  <strong>Self-hosted WebRTC video conferencing, written entirely in Rust.</strong><br />
  Leptos (WASM) frontend &middot; Axum backend &middot; shared Serde types &mdash; one Cargo workspace.
</p>

<p align="center">
  <img src="./tests/screenshots/03-room.png" alt="Juncto meeting room" width="900" />
</p>

<hr />

## Why Juncto

Juncto is an open-source video conferencing platform you can run yourself. The whole
stack &mdash; server, client and the wire types between them &mdash; is one Rust
workspace compiled to WebAssembly and a native binary. There is no JavaScript build
step, no separate SPA bundle to deploy, and no second language to keep in sync.

* **One language, one workspace.** The client, the server and the shared message types
  are Rust crates in the same repository.
* **No JS bundler.** The Leptos client is compiled to `wasm32-unknown-unknown` and served
  straight from `frontend/pkg` by the Axum server.
* **Type-safe protocol.** Every WebSocket message is a `serde` enum in the `shared`
  crate, so a client and server mismatch is a compile error, not a runtime surprise.
* **Self-contained deployment.** A single binary plus a directory of static assets.

## Features

| Area | What you get |
|---|---|
| Conferencing | HD audio/video, WebRTC mesh, mute/camera controls, raise hand |
| Layout | Tile view, speaker view with spotlight, filmstrip thumbnails, picture-in-picture |
| Collaboration | Whiteboard, polls (create/active/history), shared document, Etherpad, shared video, GIF chat |
| Messaging | Public and private chat, typing indicators, chat history for late joiners, reactions |
| Sharing | Screen share, file sharing with chat attachments, local recording |
| Security | Room lock with optional password, lobby / waiting room, E2EE indicator, visitor mode |
| Personalisation | Virtual backgrounds, device selection, dynamic branding, noise suppression toggle |
| Accessibility | Keyboard shortcuts, i18n (EN/ID), audio-level and connection-quality indicators |
| Integrations | Calendar list, Salesforce panel, iframe embed code, static dial-in info |
| Rooms | Moderator controls, breakout rooms, remote control, speaker stats, analytics, presence |

Unsupported-browser detection and a rejoin overlay keep the experience predictable when a
network drops or WebRTC is unavailable.

## Screenshots

Every view below is captured automatically from the running application by
`tests/e2e/screenshot-gallery.spec.ts`. See [docs/DEVELOPMENT.md](./docs/DEVELOPMENT.md)
for how the gallery and the contrast audit work.

### Joining a meeting

| Landing page | Pre-join | Waiting room |
|---|---|---|
| ![Landing page](./tests/screenshots/01-home.png) | ![Pre-join screen](./tests/screenshots/02-prejoin.png) | ![Lobby](./tests/screenshots/35-lobby.png) |

### In the meeting

| Meeting room | Participants | Chat |
|---|---|---|
| ![Meeting room](./tests/screenshots/03-room.png) | ![Participants panel](./tests/screenshots/04-participants.png) | ![Chat panel](./tests/screenshots/05-chat.png) |

| Notifications | Speaker stats | Whiteboard |
|---|---|---|
| ![Notifications](./tests/screenshots/13-notifications.png) | ![Speaker stats](./tests/screenshots/17-speaker-stats.png) | ![Whiteboard](./tests/screenshots/14-whiteboard.png) |

| Breakout rooms | Reactions | Subtitles |
|---|---|---|
| ![Breakout rooms](./tests/screenshots/31-breakout-rooms.png) | ![Reactions](./tests/screenshots/28-reactions.png) | ![Subtitles](./tests/screenshots/29-subtitles.png) |

| Context menu | File sharing | Visitor mode |
|---|---|---|
| ![Context menu](./tests/screenshots/30-context-menu.png) | ![File sharing](./tests/screenshots/27-files.png) | ![Visitor mode](./tests/screenshots/34-visitor-mode.png) |

### Settings

| Profile | Devices | Integrations |
|---|---|---|
| ![Settings profile](./tests/screenshots/06-settings-profile.png) | ![Settings devices](./tests/screenshots/07-settings-devices.png) | ![Settings integrations](./tests/screenshots/08-settings-integrations.png) |

| More | Branding | Virtual background |
|---|---|---|
| ![Settings more](./tests/screenshots/09-settings-more.png) | ![Settings branding](./tests/screenshots/10-settings-branding.png) | ![Virtual background](./tests/screenshots/15-virtual-background.png) |

### Collaboration and dialogs

| Polls &mdash; create | Polls &mdash; active | Invite people |
|---|---|---|
| ![Create a poll](./tests/screenshots/11-polls-create.png) | ![Active poll](./tests/screenshots/12-polls-active.png) | ![Invite people](./tests/screenshots/18-invite.png) |

| Share video | Embed meeting | Calendar |
|---|---|---|
| ![Share video](./tests/screenshots/19-share-video.png) | ![Embed meeting](./tests/screenshots/20-embed-meeting.png) | ![Calendar](./tests/screenshots/25-calendar.png) |

| Shared document | Authentication | Feedback |
|---|---|---|
| ![Shared document](./tests/screenshots/26-etherpad.png) | ![Authentication](./tests/screenshots/24-authentication.png) | ![Feedback](./tests/screenshots/23-feedback.png) |

| Dial-in info | Salesforce | Remote control |
|---|---|---|
| ![Dial-in info](./tests/screenshots/21-dial-in.png) | ![Salesforce](./tests/screenshots/22-salesforce.png) | ![Remote control](./tests/screenshots/32-remote-control.png) |

| Keyboard shortcuts | Reconnect overlay | Chat open (full view) |
|---|---|---|
| ![Keyboard shortcuts](./tests/screenshots/16-shortcuts.png) | ![Reconnect overlay](./tests/screenshots/36-rejoin-overlay.png) | ![Room with chat open](./tests/screenshots/33-room-chat-open.png) |

### Mobile (420 &times; 860)

| Landing page | Pre-join | In the meeting |
|---|---|---|
| ![Mobile landing](./tests/screenshots/37-mobile-home.png) | ![Mobile pre-join](./tests/screenshots/38-mobile-prejoin.png) | ![Mobile meeting room](./tests/screenshots/39-mobile-room.png) |

| Chat panel | Settings |
|---|---|
| ![Mobile chat](./tests/screenshots/40-mobile-chat.png) | ![Mobile settings](./tests/screenshots/41-mobile-settings.png) |

## Quick start

### Prerequisites

* A Rust toolchain (stable, 1.75+) with the WebAssembly target:
  `rustup target add wasm32-unknown-unknown`
* `wasm-bindgen-cli` at the version pinned in `Cargo.lock`
  (`cargo install wasm-bindgen-cli`)
* Node.js 18+ for the Playwright end-to-end suite
* On Linux, `pkg-config` and OpenSSL development headers
  (`apt-get install pkg-config libssl-dev`) so the host-side frontend build can link

### Build and run

```sh
bash build.sh                     # compile the WASM client into frontend/pkg
cargo run -p backend              # serve the app on http://localhost:3000
```

`build.sh` compiles the Leptos client for `wasm32-unknown-unknown`, runs
`wasm-bindgen` to emit JS/WASM bindings into `frontend/pkg`, and copies
`frontend/index.html` next to them. The Axum server serves that directory plus
`backend/static`. Open `http://localhost:3000`, type a room name, and join.

The `Makefile` wraps the same steps:

```sh
make build        # same as bash build.sh
make test         # cargo test --workspace
make test-e2e     # build + npm ci + Playwright
make clean        # cargo clean
```

### Verify your change

Run everything from the repository root:

```sh
cargo fmt --all -- --check                    # formatting
cargo clippy --workspace -- -D warnings       # lints, warnings are errors
cargo test --workspace                        # 122 unit tests
cd tests/e2e && npx playwright test           # end-to-end parity + visual suite
```

The Playwright suite boots the real backend and captures screenshots to
`tests/screenshots/`, so a failing run also tells you whether the UI regressed visually.

## Repository layout

```
.
├── backend/          Axum server: router, WebSocket handler, feature endpoints, static assets
│   ├── src/handlers/     chat, polls, whiteboard, breakout rooms, … (one module per feature)
│   └── static/           styles.css and client-side assets
├── frontend/         Leptos client-side-rendered WASM app
│   ├── src/              feature modules (chat.rs, polls.rs, whiteboard.rs, …)
│   └── src/components_ui/ reusable dialogs and panels
├── shared/           Serde types exchanged over the WebSocket
├── tests/
│   ├── e2e/              Playwright suites (parity, visual gallery, contrast audit)
│   └── screenshots/      generated PNG gallery referenced by this README
├── docs/             architecture, development and feature notes
├── build.sh          WASM + bindings build
├── Makefile          build / test / test-e2e / clean
└── Cargo.toml        the workspace manifest
```

## Documentation

| Document | Contents |
|---|---|
| [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) | Crates, message flow, WebRTC model, static asset resolution |
| [docs/DEVELOPMENT.md](./docs/DEVELOPMENT.md) | Local setup, build loop, testing, screenshot gallery |
| [docs/FEATURES.md](./docs/FEATURES.md) | Feature-by-feature status and known limitations |
| [MIGRATION.md](./MIGRATION.md) | The React → Rust cutover: gap matrix and exit criteria |
| [CONTRIBUTING.md](./CONTRIBUTING.md) | How to contribute, coding standards, commit conventions |
| [SECURITY.md](./SECURITY.md) | How to report a vulnerability |

## Configuration

The server listens on `0.0.0.0:3000`; that address is currently compiled in, so change it
in `backend/src/main.rs` and rebuild if you need a different port.

| Variable | Default | Purpose |
|---|---|---|
| `ROOM_ADMIN_TOKEN` | _(unset)_ | Bearer token required by `POST /api/rooms`. The endpoint is disabled while unset, so an anonymous client cannot reset the room. |
| `MOCK_AUTH_USER` | `admin` | Username accepted by the demo authentication dialog |
| `MOCK_AUTH_PASS` | `admin123` | Password accepted by the demo authentication dialog |

Static paths are resolved against `CARGO_MANIFEST_DIR`, so the binary serves the client
correctly no matter which directory you launch it from.

## Security

Room-level protection is available today: room lock with an optional password, a lobby
that requires host approval before anyone enters, visitor mode, and an E2EE status
indicator with per-participant key-exchange signalling.

The room-reset endpoint (`POST /api/rooms`) mutates the single global room and clears all
live state, so it requires a bearer token set via `ROOM_ADMIN_TOKEN`; while that variable
is unset the endpoint is disabled. Chat history, whiteboard strokes and polls are capped
and inbound WebSocket messages are size-limited, so a single client cannot grow server
memory without bound.

For reporting vulnerabilities, see [SECURITY.md](./SECURITY.md).

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](./CONTRIBUTING.md) for the workflow,
commit conventions and the checks your pull request must pass.

<br />

<footer>
<p align="center" style="font-size: smaller;">
Built with ❤️ by the Juncto maintainers and community.
</p>
</footer>