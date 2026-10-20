import { defineConfig } from '@playwright/test';
import * as path from 'path';
import * as crypto from 'crypto';

// Repository root (parent of tests/e2e).
const repoRoot = path.resolve(__dirname, '..', '..');

// The room-reset endpoint is state-destructive, so its token must not be a
// guessable constant checked into the repo. Generate a fresh one per run.
// Playwright may evaluate this config more than once, so stash it in the
// environment and reuse it, otherwise the server and the tests would get
// different tokens.
const adminToken = process.env.E2E_ADMIN_TOKEN ?? crypto.randomUUID();
process.env.E2E_ADMIN_TOKEN = adminToken;

export default defineConfig({
  testDir: './',
  timeout: 120000, // Increase global test timeout to 2 minutes
  retries: 2, // Retry a couple of times to absorb WebSocket timing races across shared contexts
  workers: 1, // Enforce serial execution to prevent state collisions on singleton backend
  use: {
    baseURL: 'http://localhost:3000',
    // POST /api/rooms (the state-reset endpoint the specs call between tests)
    // now requires the admin token; the webServer below is started with the
    // same value. The request fixture inherits these headers automatically.
    extraHTTPHeaders: {
      Authorization: `Bearer ${adminToken}`,
    },
    launchOptions: {
      args: [
        '--use-fake-ui-for-media-stream',
        '--use-fake-device-for-media-stream',
      ],
    },
  },
  webServer: {
    // Build the WASM frontend first, then run the backend from the repo root.
    // ROOM_ADMIN_TOKEN matches the Authorization header above so the specs can
    // reset room state; it is random per run (see `adminToken`).
    command: `export PATH=$HOME/.cargo/bin:$PATH ROOM_ADMIN_TOKEN=${adminToken} && cd ${JSON.stringify(repoRoot)} && ./build.sh && cargo run --bin backend`,
    port: 3000,
    reuseExistingServer: false,
    timeout: 300000, // Increase timeout for build
  },
});
