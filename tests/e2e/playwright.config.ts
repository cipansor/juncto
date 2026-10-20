import { defineConfig } from '@playwright/test';
import * as path from 'path';

// Repository root (parent of tests/e2e).
const repoRoot = path.resolve(__dirname, '..', '..');

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
      Authorization: 'Bearer test-admin-token',
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
    // reset room state.
    command: `export PATH=$HOME/.cargo/bin:$PATH ROOM_ADMIN_TOKEN=test-admin-token && cd ${JSON.stringify(repoRoot)} && ./build.sh && cargo run --bin backend`,
    port: 3000,
    reuseExistingServer: false,
    timeout: 300000, // Increase timeout for build
  },
});
