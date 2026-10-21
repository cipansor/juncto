# Features

Status of every feature in the Rust workspace. `MIGRATION.md` records the same matrix in
the context of the React → Rust cutover; this page is the forward-looking view, including
what is deliberately limited and why.

Legend: **complete** — implemented and exercised end to end · **partial** — works, with a
documented reduction in scope · **indicator-only** — UI and signalling exist, but no real
backend implementation · **out of scope** — deliberately not implemented.

## Conferencing

| Feature | Status | Notes |
|---|---|---|
| Video and audio | complete | WebRTC mesh via `webrtc.rs` and `media.rs` |
| Mute / camera toggles | complete | `ClientMessage::SetMuteStatus` / `SetCameraMuteStatus` |
| Device selection | complete | Microphone, camera and speaker pickers in Settings → Devices |
| Raise hand | complete | Timestamped so the roster can order hands |
| Tile view | complete | Grid layout in `components_ui/video_grid.rs` |
| Speaker view | complete | Spotlight plus filmstrip thumbnails |
| Picture-in-picture | complete | `requestPictureInPicture` on each video tile |
| Screen share | complete | `getDisplayMedia`; the browser's own picker is the UI |
| Video quality selector | complete | HD/SD preference in Settings → Devices |
| Audio level indicator | complete | `components_ui/audio_level_indicator.rs` |
| Connection quality | complete | Good/Fair/Poor badge derived from RTT (`connection_stats.rs`) |
| Local recording | partial | Records locally via `MediaRecorder`; no server-side (Jibri-style) recorder |
| Unsupported browser | complete | WebRTC capability check in `lib.rs` |
| Reconnect | complete | Overlay with a "Rejoin now" action on WebSocket drop |

## Collaboration

| Feature | Status | Notes |
|---|---|---|
| Chat | complete | Public and private messages, history replayed to late joiners |
| Typing indicator | complete | |
| Reactions | complete | |
| GIF chat | complete | `GIF:` prefix resolved through `giphy.rs` |
| Whiteboard | complete | Freehand drawing broadcast over the WebSocket |
| Polls | complete | Create, vote, close; Active and History tabs |
| File sharing | complete | Attachments ride on chat messages |
| Shared video | complete | YouTube/URL dialog, synchronised for the room |
| Shared document | partial | Embeds a configurable Etherpad URL; no bundled pad server |
| Breakout rooms | complete | Create, assign, join, close; participants see an in-room badge |
| Notifications | complete | Header bell with unread count and a history panel |

## Rooms and moderation

| Feature | Status | Notes |
|---|---|---|
| Room lock | complete | Optional password; the password never leaves the server |
| Lobby / waiting room | complete | Host approves or rejects; 120-second timeout |
| Moderator controls | complete | Mute others, kick, grant/request permissions |
| Max participants | complete | Enforced under the participants lock |
| Host election | complete | First non-visitor; reassigned if the host disappears |
| Visitor mode | complete | Visitors get a reduced toolbox and cannot become host |
| Presence | complete | `PresenceStatus` broadcast on change |
| Speaker stats | complete | Per-participant speaking time |
| Remote control | complete | Request/allow/deny signalling over the WebSocket |
| Analytics | complete | Client-side interaction tracking (`analytics.rs`) |

## Personalisation and accessibility

| Feature | Status | Notes |
|---|---|---|
| Virtual background | partial | Blur and image modes via a canvas pipeline; no rnnoise-style segmentation |
| Dynamic branding | complete | Room name, logo and colours pushed by the host |
| Noise suppression | complete | Browser constraint-based, not an rnnoise port |
| Noise / no-audio / talk-while-muted detection | complete | Surface as toasts |
| Keyboard shortcuts | complete | `shortcuts.rs` plus the dialog listing them |
| Internationalisation | complete | EN and ID strings in `i18n.rs` |
| Always on top | complete | `components_ui/always_on_top.rs` |
| Power monitor | complete | Battery/charging awareness |
| Deep linking | complete | Welcome-page redirect from a deep link |
| Recent meetings | complete | Persisted in `localStorage` |
| Screenshot capture | complete | Client-side capture for thumbnails |

## Security

| Feature | Status | Notes |
|---|---|---|
| Room password | complete | Exact-match check server-side; `has_password` is the only public hint |
| E2EE | indicator-only | Per-participant and per-room toggles plus a key-exchange message exist; **no actual encryption is performed**. Treat the indicator as a status display, not a security guarantee. |
| Authentication | partial | The login dialog talks to a **mock** credential check (`MOCK_AUTH_USER` / `MOCK_AUTH_PASS`, default `admin` / `admin123`). Replace before gating anything on it. |
| Visitor mode | complete | Read-only participation |
| Room reset | complete | `POST /api/rooms` requires the `ROOM_ADMIN_TOKEN` bearer token; disabled while unset |
| Bounded room state | complete | Chat history, whiteboard strokes and polls are capped; WebSocket messages are size-limited |

> **Do not rely on E2EE or the authentication dialog for confidentiality.** Both are
> wired through the UI and the protocol but stop short of a real cryptographic or identity
> implementation. This is stated plainly here so nobody deploys them believing otherwise.

## Integrations

| Feature | Status | Notes |
|---|---|---|
| Calendar | partial | Renders a meeting list from a handler stub; no Google OAuth |
| Salesforce | partial | Panel and link state exist; returns not-linked, no real OAuth |
| Dropbox | partial | Handler stub; no OAuth |
| Embed meeting | partial | Generates iframe embed code; no `postMessage` bridge |
| Dial-in info | partial | Displays static numbers; there is no SIP gateway behind them |

## Deliberately not implemented

| Feature | Reason |
|---|---|
| Mobile apps (iOS / Android / TWA / React Native SDK) | Out of scope; the web client is responsive down to phone widths |
| SIP / telephony gateway | Requires external infrastructure (Jigasi-style) that this project does not ship |
| Server-side transcription and subtitles | Needs an STT backend; the subtitles overlay exists but has no transcript source |
| `rtcstats` debug pipeline | Low value for the self-hosted use case |
| Web HID integration | No default hardware requirement |
| Custom panel (arbitrary UI injection) | Security risk for marginal benefit |
| Chrome extension banner, legacy-client notice | Irrelevant after the rewrite |
| Public `postMessage` command/event API | The iframe embed covers the documented use case |
| Enterprise SSO / JWT sessions | Anonymous joining is the only supported mode, matching the previous client |

## Known limitations

* **Mesh topology.** Every participant connects to every other participant, so upload
  bandwidth scales with room size. Comfortable for small meetings; an SFU would be needed
  for large ones.
* **In-memory state.** Rooms, chat history and whiteboard strokes live in the process. A
  restart clears them, and there is no horizontal scaling or persistence layer.
* **Media devices in CI.** The Playwright suite runs Chromium with fake media devices, so
  it verifies wiring and UI, not real codec behaviour.
* **Two skipped Playwright specs.** A small number of specs are skipped by design where
  the behaviour depends on hardware or external services.

## Where to look next

* [ARCHITECTURE.md](./ARCHITECTURE.md) — how the crates, protocol and media path fit
  together.
* [DEVELOPMENT.md](./DEVELOPMENT.md) — build, test and screenshot workflow.
* [MIGRATION.md](../MIGRATION.md) — the full cutover matrix with per-feature evidence.