import { test, expect } from '@playwright/test';

// The room-reset endpoint is state-destructive (it wipes the single global
// room), so it must reject anonymous callers. The token is configured in
// playwright.config.ts for the specs that legitimately reset state.
test.describe('Security hardening', () => {
    const body = {
        room_name: 'SecurityHardening',
        is_locked: false,
        is_recording: false,
        is_lobby_enabled: false,
        max_participants: 10,
    };

    test('POST /api/rooms rejects an unauthenticated reset', async () => {
        // Use plain fetch: a Playwright request context would inherit the
        // Authorization header configured for the rest of the suite.
        const response = await fetch('http://localhost:3000/api/rooms', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body),
        });
        expect(response.status).toBe(401);
    });

    test('POST /api/rooms rejects a wrong token', async ({ playwright }) => {
        const wrongToken = await playwright.request.newContext({
            baseURL: 'http://localhost:3000',
            extraHTTPHeaders: { Authorization: 'Bearer not-the-token' },
        });
        try {
            const response = await wrongToken.post('/api/rooms', { data: body });
            expect(response.status()).toBe(401);
        } finally {
            await wrongToken.dispose();
        }
    });

    test('POST /api/rooms accepts the configured token', async ({ request }) => {
        const response = await request.post('/api/rooms', { data: body });
        expect(response.status()).toBe(201);
    });

    test('POST /api/rooms accepts a case-insensitive bearer scheme', async ({ playwright }) => {
        // RFC 7235 scheme names are case-insensitive; "bearer" must work.
        const lower = await playwright.request.newContext({
            baseURL: 'http://localhost:3000',
            extraHTTPHeaders: { Authorization: 'bearer ' + process.env.E2E_ADMIN_TOKEN },
        });
        try {
            const response = await lower.post('/api/rooms', { data: body });
            expect(response.status()).toBe(201);
        } finally {
            await lower.dispose();
        }
    });

    test('POST /api/rooms rejects invalid branding', async ({ request }) => {
        const response = await request.post('/api/rooms', {
            data: {
                ...body,
                branding: { primary_color: 'url(https://evil.example/x)' },
            },
        });
        expect(response.status()).toBe(400);
    });

    test('display name input enforces the 64-character limit', async ({ page }) => {
        await page.goto('/room/e2e-test-room');
        const input = page.locator('#display-name');
        await expect(input).toBeVisible();
        await expect(input).toHaveAttribute('maxlength', '64');
        await input.fill('x'.repeat(100));
        await expect(input).toHaveValue('x'.repeat(64));
    });
});
