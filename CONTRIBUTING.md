# Contributing to Juncto

Thanks for wanting to help. This page covers everything you need to get a change from an
idea to a merged pull request. The repository is a single Rust workspace — there is no
JavaScript source tree, only the Playwright suite under `tests/e2e`.

## Getting set up

Install the toolchain:

| Tool | Install |
|---|---|
| Rust (stable) | `curl --proto '=http' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| WebAssembly target | `rustup target add wasm32-unknown-unknown` |
| `wasm-bindgen-cli` | `cargo install wasm-bindgen-cli` (match the version in `Cargo.lock`) |
| Node.js 18+ | Your package manager, for the Playwright suite |
| `pkg-config` + OpenSSL headers | `apt-get install pkg-config libssl-dev` on Debian/Ubuntu |

Then build and run:

```sh
bash build.sh            # compile the WASM client into frontend/pkg
cargo run -p backend     # serve http://localhost:3000
```

[DEVELOPMENT.md](./docs/DEVELOPMENT.md) goes deeper on the build loop, debugging and the
test suites.

## Before you open a pull request

Run all four from the repository root and make sure they pass:

```sh
cargo fmt --all -- --check                 # formatting
cargo clippy --workspace -- -D warnings    # lints; a warning fails the build
cargo test --workspace                     # 130 unit tests
cd tests/e2e && npx playwright test        # end-to-end suite
```

CI runs the same checks in a `checks` job followed by an `e2e` job, so a local pass means a
green build.

## What to work on

Good first contributions tend to be small and well-bounded:

* Fixing a bug with a reproduction in the issue.
* Adding a Playwright spec for behaviour that currently has none.
* Improving accessibility, contrast or responsive behaviour.
* Filling a gap listed as **partial** or **indicator-only** in
  [docs/FEATURES.md](./docs/FEATURES.md).

If you are planning something larger — a new protocol message, a change to the WebRTC
model, a new dependency — open an issue first so the approach can be agreed before you
write the code.

## Coding standards

### Commits

Use [Conventional Commits](https://www.conventionalcommits.org) with a scope:

```
feat(polls): add a history tab
fix(chat): replay history to late joiners
docs(readme): embed the screenshot gallery
```

Types in use: `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`,
`style`, `test`.

### Rust

* Formatting and lints are enforced (`cargo fmt`, `clippy -D warnings`).
* Add unit tests beside the code under `#[cfg(test)] mod tests`.
* Keep modules focused. Feature logic belongs in its own module under `frontend/src/` or
  `backend/src/handlers/`, not bolted onto `pages/room.rs` or `handlers/ws.rs`.
* The `shared` crate is the protocol contract. Adding a variant means handling it on both
  sides; let the compiler guide you.

### CSS

Style with the custom properties defined on `:root` in `backend/static/styles.css`
(`--bg-surface`, `--text-primary`, `--primary-color`, `--radius-*`, `--shadow-*`, …).
Literal colours inside a dialog are how the near-white-on-white bugs appeared; the contrast
audit will now fail the build for them.

### Tests

* Logic change → unit test.
* UI behaviour change → Playwright spec in `tests/e2e/`.
* New or changed screen → update `screenshot-gallery.spec.ts` and the README tables.
* CSS or dialog markup change → run `contrast-audit.spec.ts`.

Look at any screenshot you generate. A blank, white or unstyled capture is the specific
failure this tooling exists to catch: fix the UI, then regenerate.

## Submitting a pull request

1. Branch from `main` with a descriptive name (`fix/chat-history-replay`).
2. Keep the change focused. Unrelated refactors make review harder and are better as a
   separate pull request.
3. Fill in the pull request template, including which checks you ran.
4. Update documentation when behaviour, setup or features change — `README.md`,
   `docs/FEATURES.md` and `docs/ARCHITECTURE.md` are the usual candidates.
5. Be responsive to review comments. If you disagree with feedback, say so with reasoning;
   that is a normal and useful part of the process.

## Reporting bugs

A useful bug report includes:

* What you did, what you expected, and what actually happened.
* The browser and version, and whether the server was built with `build.sh` at that commit.
* Browser console output — WASM load failures and WebSocket errors both surface there.
* A screenshot if the problem is visual. If you can, run
  `npx playwright test screenshot-gallery.spec.ts` and attach the relevant capture.

## Security

Do not report security issues in a public issue or pull request. See [SECURITY.md](./SECURITY.md)
for the private reporting channel: GitHub private vulnerability reporting, tracked on the
repository's Security Advisories page.

By contributing, you agree that your contributions will be licensed under the Apache License,
Version 2.0, and that the copyright is held by Yayasan Pesantren Cipansor and the juncto
contributors.

## Community

* [Juncto Handbook](https://juncto.github.io/handbook/docs/dev-guide/dev-guide-contributing/)
  — the upstream contribution guide
* [Community Forum](https://community.juncto.org/) — questions and support