# CLAUDE.md

Guidance for Claude Code (and other AI agents) working in this repository.

> **Cutover note:** the React/Webpack web codebase, the mobile apps, and the server
> packaging infrastructure have been deleted. The repository root is now a single Rust
> workspace: a Leptos (WASM) frontend, an Axum backend, and a `shared` crate holding the
> WebSocket types. There is no JavaScript source to edit — the only Node project is the
> Playwright suite.

## Repository layout

| Path | What it contains |
|---|---|
| `backend/` | Axum server. `src/main.rs` builds the router and state; `src/handlers/` holds one module per feature; `src/api.rs` re-exports the route handlers; `static/` holds the stylesheet |
| `frontend/` | Leptos client-side-rendered WASM app. Feature modules at `src/`, reusable widgets under `src/components_ui/`, pages under `src/pages/` |
| `shared/` | `serde` types for both sides of the WebSocket (`src/lib.rs`) |
| `tests/e2e/` | Playwright suites, including the screenshot gallery and the contrast audit |
| `tests/screenshots/` | Generated PNG gallery referenced by the README |
| `docs/` | Architecture, development and feature documentation |
| `build.sh`, `Makefile`, `Cargo.toml` | Build tooling and the workspace manifest |

## Commands

Run everything from the repository root.

```sh
bash build.sh                            # WASM + bindings → frontend/pkg, copies index.html
cargo run -p backend                     # serves http://localhost:3000
cargo test --workspace                   # 102 unit tests
cargo fmt --all -- --check               # formatting check
cargo clippy --workspace -- -D warnings  # lints (warnings are errors)

cd tests/e2e && npx playwright test                        # full e2e suite
cd tests/e2e && npx playwright test screenshot-gallery.spec.ts   # regenerate README images
cd tests/e2e && npx playwright test contrast-audit.spec.ts       # dialog legibility audit
```

Environment notes:

* `pkg-config` and OpenSSL headers are required because the host build of `frontend` links
  OpenSSL through `reqwest`. Without them `cargo test` fails with an `openssl-sys` error.
  Use `cargo test -p shared` or `-p backend` to avoid building the frontend for the host.
* The Playwright suite refuses to start if something already listens on port 3000.

## Architecture in one minute

One WebSocket at `/ws/chat` carries everything: chat, polls, whiteboard strokes, breakout
transitions, moderation, remote-control signalling and WebRTC SDP/ICE. Messages are
`shared::ClientMessage` and `shared::ServerMessage` — a single Rust enum each, compiled
into both the WASM client and the native server, so protocol drift is a compile error.

* Media is a peer-to-peer WebRTC mesh; the server only relays signalling and room events.
* Room state is in-memory behind mutexes, fanned out over a `tokio::sync::broadcast`
  channel, plus a direct channel per socket for messages addressed to one client.
* Static directories resolve against `env!("CARGO_MANIFEST_DIR")`, so the binary serves the
  client regardless of the process working directory.
* The client is Leptos CSR (no SSR). One `active_panel` signal makes the chat,
  participants and files panels mutually exclusive; at ≤ 768 px the panel starts closed.

See `docs/ARCHITECTURE.md` for the full picture.

## Conventions

### Commit messages

[Conventional Commits](https://www.conventionalcommits.org) with a scope:

```
feat(chat): add message reactions
fix(webrtc): negotiate ICE before SDP answer
docs(readme): document the screenshot gallery
```

Types in use: `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`,
`style`, `test`.

### Rust

* Run `cargo fmt` and `cargo clippy --workspace -- -D warnings` before committing.
* Put unit tests in a `#[cfg(test)] mod tests` block beside the code they cover.
* When you add a `ClientMessage` or `ServerMessage` variant, handle it explicitly in the
  dispatch `match` in `backend/src/handlers/ws.rs` — the compiler will point you there.
* Prefer adding a module under `frontend/src/` (or `components_ui/`) over growing
  `pages/room.rs`, which already composes a large number of dialogs.

### Styling

All CSS lives in `backend/static/styles.css`, driven by the custom properties on `:root`
(`--bg-surface`, `--text-primary`, `--primary-color`, `--radius-*`, `--shadow-*`, …). Use
those tokens rather than literal colours: hardcoded hex values are what produced the
near-white-on-white dialog bugs, and the contrast audit will now fail the build for them.
Responsive rules belong in the `@media` blocks at the bottom of the file (768 px, 480 px).
A rule that has to react to a panel's width rather than the window's is a `@container`
query instead — the room header is 440 px wide at a 780 px viewport with the chat panel
open — and lives with those blocks, not in the element's own section.

A badge or count that sits inside a button must not change that button's width: the
header reserves a fixed corner for the bell, and a flow child grew it from 30 px to
56 px, which put a two-digit count 4 px into the lock badge. Position such a child
absolutely so the reserved width bounds the whole control however large the number gets.

## Testing expectations

* A pure-logic change should come with a unit test.
* A UI behaviour change should come with a Playwright spec in `tests/e2e/`.
* A new screen should be added to `screenshot-gallery.spec.ts` and referenced from the
  README. Keep the numbered file-name ordering intact.
* If you change CSS or dialog markup, run `contrast-audit.spec.ts`.
* Never commit a screenshot you have not looked at. Blank, white or unstyled captures are
  the failure mode this suite exists to catch — regenerate and fix the UI first.

## CI

`.github/workflows/rust-ci.yml` runs two jobs:

1. `checks` — `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`,
   `cargo test --workspace`.
2. `e2e` — `build.sh` plus the Playwright suite.

Dependabot tracks the `cargo` ecosystem and the `npm` ecosystem used by `tests/e2e`.

## External resources

* [Juncto Handbook](https://juncto.github.io/handbook/) — user and developer documentation
* [Community Forum](https://community.juncto.org/) — questions and support
* [Contributing Guidelines](https://juncto.github.io/handbook/docs/dev-guide/dev-guide-contributing/) — the upstream contribution process