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

`cargo test --workspace` currently runs **130 tests** across the three crates: 30 in
`shared`, 49 in `frontend` (3 ignored) and 51 in `backend`. Tests live beside the code they
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

The file names are ordered and stable (`01-home.png` … `42-settings-moderator.png`). If you
add a screen, append the next number; if you renumber, update the README tables in the same
commit.

Every capture goes through a `capture()` helper that records the name as it is written, and
two assertions run in `afterAll`:

* **No duplicate numbers** — a repeated prefix makes the gallery order ambiguous and
  collides in the README tables. Checking the output directory cannot catch this (a clean
  checkout has no files, and two same-named captures leave one file behind), so the names
  are recorded as they are written instead.
* **No missing screens** — `REQUIRED_SHOTS` lists every view the gallery must produce.
  Several captures are conditional (the lobby, the reconnect overlay, the context menu,
  the mobile settings dialog) and skip themselves when the state cannot be reached; without
  the manifest a skipped capture is simply absent and the run still passes, leaving a
  broken image in the README.

Screens that depend on timing are still skipped with an annotation rather than failing on
the spot, but a skip now fails the run at the end. When that happens, the fix is to make the
state reachable, not to drop the entry from `REQUIRED_SHOTS`.

Two of the captures need setup worth knowing about:

* **Lobby (35)** — the lobby only engages when a host is already in the room, so the test
  joins a host first and only then lets the second client knock.
* **Reconnect overlay (36)** — `context.setOffline(true)` does not reliably tear down an
  already-open WebSocket, so the test installs an init script that records every
  `WebSocket` instance and closes them directly to force the drop.

### The contrast audit

`contrast-audit.spec.ts` opens every dialog and measures the contrast ratio between each
run of text and the surface actually painted behind it. It fails when any dialog drops below
the WCAG AA floor (3:1 for large text — the headings and labels here) or renders on a
near-white surface.

The shared helper (`contrast.ts`) samples the rendered pixels rather than reconstructing the
backdrop from CSS, because a `background-color` walk cannot see what the browser composites:
the landing page paints a radial gradient in `background-image` behind a translucent card
with `backdrop-filter`, so a reconstructed colour would be the card over the body, not the
card over the gradient users see. Four details make the sample honest:

* **Text hidden before sampling.** The screenshot is taken with
  `-webkit-text-fill-color: transparent` on the audited subtree, so every pixel in a line
  box is the surface behind the words. The earlier approach kept the glyphs and discarded
  pixels that resembled the text colour, which cannot be made sound: a backdrop matching
  the text colour *is* the unreadable case, so the filter deletes the evidence. White text
  on a mostly-white line kept only the dark patch beside it and scored 21:1 while most of
  the letters were invisible. `-webkit-text-fill-color` is used rather than `color:
  transparent` because it leaves backgrounds, borders and shadows alone — a
  `background: currentColor` would otherwise vanish and change the surface being measured.
* **Per text run.** Colour is read per text node, not from the audited element. The audit
  target is often a container — `.chat-container` holds a white heading, a muted label and a
  white-on-blue button — so the container's own colour (black, inherited) scored against the
  dark card would report 1.44:1 for a container whose text is all legible.
* **Per line box.** The sample is restricted to the text's own line boxes, so a short
  message in a wide paragraph is measured where the glyphs are, not where the box happens
  to be empty. A gradient crossing the text yields one surface per line and the worst line
  decides.
* **Every colour under the glyphs, worst wins.** Within a line, each colour painted under
  the letters is scored and the lowest ratio is reported. There is no minimum share: a
  narrow unreadable band is exactly the case that matters, and a threshold discards
  precisely that. A share also depends on how wide the font renders, so a thresholded
  audit can change verdict between machines.

Only rows inside the screenshot are read. A line below the fold has no pixels to measure,
and reading past the last row yields `undefined` channels and a `NaN` ratio. A `NaN` does
not survive to the assertion — `toBeGreaterThanOrEqual` rejects it — but it can vanish
first, because the worst-score selection compares with `<`, which is false for `NaN`, so a
`NaN` line is skipped and never becomes the reported worst. The line would then be counted
as measured without being scored. Offscreen lines are skipped explicitly, and an element
with nothing visible to measure fails with `no visible text line to measure`.

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