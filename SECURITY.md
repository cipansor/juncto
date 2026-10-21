# Security Policy

We take security seriously and develop all Juncto projects to be secure and safe.

## Reporting a vulnerability

Do not disclose security issues publicly.

Report vulnerabilities through GitHub private vulnerability reporting, and track
the resulting advisory on the repository's Security Advisories page:

* **Report:** https://github.com/cipansor/juncto/security/advisories/new
* **Advisories:** https://github.com/cipansor/juncto/security/advisories

Please do not use public issues, pull requests, email or any other channel.

Please include enough detail to reproduce the issue: the affected version or commit, the
steps you took, and the impact you believe it has. We encourage responsible disclosure, so
please reach out before posting in a public space, and give us a reasonable window to
investigate and ship a fix.

## Scope

Good to know when assessing this codebase:

* Because the client and server compile from the same Rust workspace, anything reachable
  from the WebSocket handler in `backend/src/handlers/ws.rs` is in scope.
* Room-level protections are implemented server-side: the room lock (with an optional
  password), the lobby approval flow, visitor mode, and the host-election rules. The room
  password is marked `skip_serializing` on `RoomConfig`, so it never travels to clients.
* **Out of scope for confidentiality guarantees today:** the E2EE indicator and the
  authentication dialog are wired through the protocol but do not perform real encryption
  or identity verification. See [docs/FEATURES.md](./docs/FEATURES.md) for the exact
  status. Treat them as UI status only.

## Supported versions

Juncto is developed on the default branch. Fixes land there and are expected to be
deployed from a current commit; older snapshots do not receive backports.