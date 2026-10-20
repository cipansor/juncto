# Development

Everything runs from the repository root. There is one Rust workspace and one Node
project (the Playwright suite in `tests/e2e`).

## Prerequisites

| Tool | Why | Install |
|---|---|---|
| Rust (stable) | Compiles all three crates | `curl --proto '=http' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| `wasm32-unknown-unknown` target | Compiles the client to WebAssembly | `rustup target add wasm32-unknown-unknown` |
| `wasm-bindgen-cli` | Generates the JS/WASM bindings | `cargo install wasm-bindgen-cli` |
| Node.js 18+ | Runs the Playwright suite | your package manager |
| `pkg-config` + OpenSSL headers | The host build of `frontend` links against OpenSSL through `reqwest` | `apt-get install pkg-config libssl-dev` |

`wasm-bindgen-cli` must match the `wasm-bindgen` version in `Cargo.lock`. A mismatch shows
up as a `schema version` error from `wasm-bindgen`; install with
`cargo install wasm-bindgen-cli --version <version-from-Cargo.lock>` if that happens.

## Build loop

```sh
bash build.sh                 # WASM + bindings → frontend/pkg, then copies index.html
cargo run -p backend          # serves http://localhost:3000
```

`build.sh` is not incremental in any clever way — it runs `cargo build` and
`wasm-bindgen`. During iteration you can skip it when you have only touched server code:

```sh
cargo run -p backend          # fast; no WASM rebuild
```

Rerun `bash build.sh` whenever you change anything under `frontend/`. The client is a
static asset, so the browser needs a hard reload (or a disabled cache) to pick up a new
`frontend.wasm`.

The `Makefile` mirrors these steps:

```sh
make build      # bash build.sh
make test       # cargo test --workspace
make test-e2e   # build + npm ci + npx playwright test
make clean      # cargo clean
```

## Checks before pushing

```sh
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cd tests/e2e && npx playwright test
```

`clippy` runs with `-D warnings`, so a new warning fails the build. CI (see
`.github/workflows/rust-ci.yml`) runs the same commands in a `checks` job followed by an
`e2e` job.

### Unit tests

`cargo test --workspace` currently runs **117 tests** across the three crates: 27 in
`shared`, 52 in `frontend` (3 ignored) and 38 in `backend`. Tests live beside the code they
exercise in a `#[cfg(test)] mod tests` block. Pure logic (message formatting, timers, ordering,
permission decisions) is the sweet spot for unit tests; anything that needs a DOM or a
socket belongs in the Playwright suite instead.

Note that a plain `cargo test` also builds the `frontend` crate for the host target, which
is why the OpenSSL headers are needed even if you only intend to run backend tests. To
skip that, target a single crate:

```sh
cargo test -p shared
cargo test -p backend
```

## End-to-end tests

All suites live in `tests/e2e/` and are Playwright specs. They drive the real application;
nothing is mocked at the API level.

```sh
cd tests/e2e
npm ci                 # first time only
npx playwright test    # everything
npx playwright test screenshot-gallery.spec.ts   # just the visual gallery
npx playwright test contrast-audit.spec.ts       # just the contrast audit
```

`playwright.config.ts` boots the app for you:

```ts
webServer: {
  command: `export PATH=$HOME/.cargo/bin:$PATH && cd ${repoRoot} && ./build.sh && cargo run --bin backend`,
  port: 3000,
  reuseExistingServer: false,
  timeout: 300000,
}
```

Two consequences to keep in mind:

* If a server is already listening on port 3000, the run aborts with
  *"http://localhost:3000 is already used"*. Stop your manual `cargo run` first.
* The job runs serially (`workers: 1`) because the backend keeps room state in memory. Two
  specs sharing a room name would interfere with each other.

Because the server resolves static paths from `CARGO_MANIFEST_DIR`, the tests can run the
binary from any working directory — that is what makes the `webServer` command above work
without `cd`-ing into the repository root for the request phase.

### The screenshot gallery

`screenshot-gallery.spec.ts` is a single long test that walks the whole UI and writes one
PNG per screen to `tests/screenshots/`. It is the source of every image in the README, so
**regenerating the gallery is how you refresh the README screenshots**:

```sh
cd tests/e2e && npx playwright test screenshot-gallery.spec.ts
```

The file names are ordered and stable (`01-home.png` … `41-mobile-settings.png`). If you
add a screen, append the next number; if you renumber, update the README tables in the same
commit. Screens that depend on timing (the lobby, the reconnect overlay) are captured
behind an explicit visibility check and are skipped with an annotation rather than failing
the suite when the state cannot be reached.

Two of the captures need setup worth knowing about:

* **Lobby (35)** — the lobby only engages when a host is already in the room, so the test
  joins a host first and only then lets the second client knock.
* **Reconnect overlay (36)** — `context.setOffline(true)` does not reliably tear down an
  already-open WebSocket, so the test installs an init script that records every
  `WebSocket` instance and closes them directly to force the drop.

### The contrast audit

`contrast-audit.spec.ts` opens every dialog and measures the contrast ratio between the
resolved text colour and the effective backdrop, computed by compositing translucent
backgrounds up the ancestor chain. It fails when any dialog drops below the WCAG AA floor
(3:1 for large text — the headings and labels here) or renders on a near-white surface.

This exists because the React → Rust cutover left several dialogs with browser-default text
on a white panel: legible in a diff, invisible on screen, and easy to miss even in a
screenshot. Run it after any CSS or markup change:

```sh
cd tests/e2e && npx playwright test contrast-audit.spec.ts
```

It prints a per-dialog ratio table on the way through, which is handy when you are
adjusting a specific panel.

## Stylesheet and design tokens

All styling lives in `backend/static/styles.css`; the Leptos components carry almost no
inline styles. It is driven by CSS custom properties on `:root`:

| Token | Role |
|---|---|
| `--bg-dark`, `--bg-surface` | Page and panel backgrounds |
| `--text-primary`, `--text-secondary`, `--text-muted` | Text hierarchy |
| `--border-color`, `--border-strong` | Hairlines and dividers |
| `--primary-color` | Accent |
| `--radius-*`, `--shadow-*` | Shape and elevation |
| `--glass-backdrop` | Overlay blur |

Prefer these tokens over literal colours so a dialog inherits dark-mode-correct text
automatically. Literal hex values are how the near-white-on-white bugs appeared in the
first place. Responsive rules live in the `@media` blocks at the bottom of the file
(768 px and 480 px).

## Debugging

**The page is blank.** Check the browser console first. A `wasm-bindgen` version mismatch
and a stale `frontend/pkg` are the two usual causes; rerun `bash build.sh` and hard-reload.

**Styles or scripts 404.** Confirm `frontend/pkg/index.html` exists. The server falls back
to `index.html` for unknown paths, so a missing build shows up as an HTML shell with no
WASM.

**A Playwright spec times out on a WebSocket assertion.** The backend is a singleton with
in-memory state; a previous spec may have left a room locked or a lobby enabled. Use a
unique room name per test (`\`Room_${Date.now()}\``) and reset the room through
`POST /api/rooms` before joining.

**`cargo test` fails with an OpenSSL error.** Install `pkg-config` and `libssl-dev`, or
target a single crate with `-p`.

## Adding a feature end to end

1. **Type the message.** Add the variants to `ClientMessage` / `ServerMessage` in
   `shared/src/lib.rs`. The compiler now tells you every place that must handle them.
2. **Handle it on the server.** Put the logic in a `backend/src/handlers/` module and
   dispatch it from the `match` in `handlers/ws.rs`.
3. **Wire the client.** Add the signals in `frontend/src/state.rs`, map incoming messages
   in `state_handlers.rs`, and build the UI in a feature module or `components_ui/`.
4. **Style it with tokens.** Use the custom properties above; do not introduce literal
   colours inside a dialog.
5. **Test it.** A unit test for the pure logic, plus a Playwright spec for the flow. Add
   the screen to `screenshot-gallery.spec.ts` and reference it from the README.
6. **Update the docs.** Note the feature in [FEATURES.md](./FEATURES.md) and, if the
   architecture shifted, in [ARCHITECTURE.md](./ARCHITECTURE.md).