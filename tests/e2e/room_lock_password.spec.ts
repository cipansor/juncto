import { expect, test } from '@playwright/test';

test.describe('Room lock password parity (Step 4)', () => {
    test('password-protected lock rejects, then admits, joins with the password', async ({
        browser,
        request,
    }) => {
        const roomResp = await request.post('http://localhost:3000/api/rooms', {
            data: {
                room_name: 'LockPwd',
                is_locked: false,
                is_recording: false,
                is_lobby_enabled: false,
                max_participants: 10,
            },
        });
        const roomName = (await roomResp.json()).config.room_name;
        const roomUrl = `/room/${encodeURIComponent(roomName)}`;

        const hostCtx = await browser.newContext();
        const guestCtx = await browser.newContext();

        try {
            const hostPage = await hostCtx.newPage();
            await hostPage.goto(roomUrl);
            await hostPage.fill('.prejoin-container input[type="text"]', 'Host');
            await hostPage.click('button.join-btn');
            await expect(hostPage.locator('.room-container')).toBeVisible();

            // Host: open settings → moderator tab, set password, lock the room
            await hostPage.click('#settings-btn');
            await hostPage.click('button:has-text("Moderator")');
            await hostPage.fill('#lock-password-input', 's3cret');
            await hostPage.click('#lock-room-toggle');
            await hostPage.click('#close-settings-btn');

            // Guest joins without password → rejected with prompt
            const guestPage = await guestCtx.newPage();
            await guestPage.goto(roomUrl);
            await guestPage.fill('.prejoin-container input[type="text"]', 'Guest');
            await guestPage.click('button.join-btn');
            await expect(guestPage.locator('.toast-container .toast-error'))
                .toHaveText('Password required', { timeout: 10000 });
            await expect(guestPage.locator('#room-password')).toBeVisible();

            // Wrong password → rejected again
            await guestPage.fill('#room-password', 'wrong');
            await guestPage.click('button.join-btn');
            await expect(
                guestPage.locator('.toast-container .toast-error').filter({
                    hasText: 'Invalid room password',
                }),
            ).toBeVisible({ timeout: 10000 });

            // Correct password → admitted
            await guestPage.fill('#room-password', 's3cret');
            await guestPage.click('button.join-btn');
            await expect(guestPage.locator('.room-container')).toBeVisible({ timeout: 10000 });
            await expect(hostPage.locator('.participant-item')).toHaveCount(2, {
                timeout: 10000,
            });
        } finally {
            await hostCtx.close();
            await guestCtx.close();
        }
    });

    test('hard lock without password rejects joins outright', async ({
        browser,
        request,
    }) => {
        const roomResp = await request.post('http://localhost:3000/api/rooms', {
            data: {
                room_name: 'HardLock',
                is_locked: false,
                is_recording: false,
                is_lobby_enabled: false,
                max_participants: 10,
            },
        });
        const roomName = (await roomResp.json()).config.room_name;
        const roomUrl = `/room/${encodeURIComponent(roomName)}`;

        const hostCtx = await browser.newContext();
        const guestCtx = await browser.newContext();

        try {
            const hostPage = await hostCtx.newPage();
            await hostPage.goto(roomUrl);
            await hostPage.fill('.prejoin-container input[type="text"]', 'Host');
            await hostPage.click('button.join-btn');
            await expect(hostPage.locator('.room-container')).toBeVisible();

            // Host locks with no password
            await hostPage.click('#settings-btn');
            await hostPage.click('button:has-text("Moderator")');
            await hostPage.click('#lock-room-toggle');
            await hostPage.click('#close-settings-btn');

            const guestPage = await guestCtx.newPage();
            await guestPage.goto(roomUrl);
            await guestPage.fill('.prejoin-container input[type="text"]', 'Guest');
            await guestPage.click('button.join-btn');
            await expect(guestPage.locator('.toast-container .toast-error'))
                .toHaveText('Room is locked', { timeout: 10000 });
            // No password prompt for a hard lock
            await expect(guestPage.locator('#room-password')).toBeHidden();
        } finally {
            await hostCtx.close();
            await guestCtx.close();
        }
    });
});

test.describe('Setting a password after locking (Step 5)', () => {
    test.beforeEach(async ({ request }) => {
        // One global room per server: clear lock/password left by earlier tests.
        const resp = await request.post('/api/rooms', {
            data: {
                room_name: 'Reset Room',
                is_locked: false,
                is_recording: false,
                is_lobby_enabled: false,
                max_participants: 100,
                host_id: null,
                e2ee_enabled: false,
            },
        });
        expect(resp.ok()).toBeTruthy();
    });

    // Locking is a server round-trip: wait for the room to report itself locked
    // before touching the password field, otherwise `Apply` is still disabled.
    const lockRoomWithoutPassword = async (hostPage: import('@playwright/test').Page) => {
        await hostPage.click('#settings-btn');
        await hostPage.click('button:has-text("Moderator")');
        await hostPage.click('#lock-room-toggle');
        await expect(hostPage.locator('.settings-hint').filter({
            hasText: 'locked without a password',
        })).toBeVisible({ timeout: 10000 });
    };

    test('password applied after locking lets a guest in', async ({ browser, request }) => {
        const roomResp = await request.post('http://localhost:3000/api/rooms', {
            data: {
                room_name: 'LatePassword',
                is_locked: false,
                is_recording: false,
                is_lobby_enabled: false,
                max_participants: 10,
            },
        });
        const roomName = (await roomResp.json()).config.room_name;
        const roomUrl = `/room/${encodeURIComponent(roomName)}`;

        const hostCtx = await browser.newContext();
        const guestCtx = await browser.newContext();

        try {
            const hostPage = await hostCtx.newPage();
            await hostPage.goto(roomUrl);
            await hostPage.fill('.prejoin-container input[type="text"]', 'Host');
            await hostPage.click('button.join-btn');
            await expect(hostPage.locator('.room-container')).toBeVisible();

            await lockRoomWithoutPassword(hostPage);

            await hostPage.fill('#lock-password-input', 'meeting42');
            // The draft is visibly unsaved until applied.
            await expect(hostPage.locator('#password-unsaved')).toBeVisible();
            await hostPage.click('#apply-password-btn');
            await expect(hostPage.locator('#password-unsaved')).toBeHidden();
            await hostPage.click('#close-settings-btn');

            const guestPage = await guestCtx.newPage();
            await guestPage.goto(roomUrl);
            await guestPage.fill('.prejoin-container input[type="text"]', 'Guest');
            await guestPage.click('button.join-btn');

            // Now a prompt, not an outright rejection.
            await expect(guestPage.locator('.toast-container .toast-error'))
                .toHaveText('Password required', { timeout: 10000 });
            await expect(guestPage.locator('#room-password')).toBeVisible();

            await guestPage.fill('#room-password', 'meeting42');
            await guestPage.click('button.join-btn');
            await expect(guestPage.locator('.room-container')).toBeVisible({ timeout: 10000 });
        } finally {
            await hostCtx.close();
            await guestCtx.close();
        }
    });

    test('Enter applies the password, Escape does not discard it', async ({ browser, request }) => {
        const roomResp = await request.post('http://localhost:3000/api/rooms', {
            data: {
                room_name: 'EnterPassword',
                is_locked: false,
                is_recording: false,
                is_lobby_enabled: false,
                max_participants: 10,
            },
        });
        const roomName = (await roomResp.json()).config.room_name;
        const roomUrl = `/room/${encodeURIComponent(roomName)}`;

        const hostCtx = await browser.newContext();
        const guestCtx = await browser.newContext();

        try {
            const hostPage = await hostCtx.newPage();
            await hostPage.goto(roomUrl);
            await hostPage.fill('.prejoin-container input[type="text"]', 'Host');
            await hostPage.click('button.join-btn');
            await expect(hostPage.locator('.room-container')).toBeVisible();

            await lockRoomWithoutPassword(hostPage);

            // Type, then submit with Enter, then leave via Escape — the exact
            // sequence that used to drop the password on Firefox.
            await hostPage.fill('#lock-password-input', 'enterkey');
            await hostPage.press('#lock-password-input', 'Enter');
            await expect(hostPage.locator('#password-unsaved')).toBeHidden();

            await hostPage.press('#lock-password-input', 'Escape');
            await expect(hostPage.locator('#lock-password-input')).toBeHidden();

            const guestPage = await guestCtx.newPage();
            await guestPage.goto(roomUrl);
            await guestPage.fill('.prejoin-container input[type="text"]', 'Guest');
            await guestPage.click('button.join-btn');
            // The prompt only renders once the server answers "Password required".
            await expect(guestPage.locator('#room-password')).toBeVisible({ timeout: 10000 });
            await guestPage.fill('#room-password', 'enterkey');
            await guestPage.click('button.join-btn');
            await expect(guestPage.locator('.room-container')).toBeVisible({ timeout: 10000 });
        } finally {
            await hostCtx.close();
            await guestCtx.close();
        }
    });
    test('reopening the dialog does not claim the room has no password', async ({
        browser,
        request,
    }) => {
        const roomResp = await request.post('http://localhost:3000/api/rooms', {
            data: {
                room_name: 'ReopenPassword',
                is_locked: false,
                is_recording: false,
                is_lobby_enabled: false,
                max_participants: 10,
            },
        });
        const roomName = (await roomResp.json()).config.room_name;
        const roomUrl = `/room/${encodeURIComponent(roomName)}`;

        const hostCtx = await browser.newContext();
        try {
            const hostPage = await hostCtx.newPage();
            await hostPage.goto(roomUrl);
            await hostPage.fill('.prejoin-container input[type="text"]', 'Host');
            await hostPage.click('button.join-btn');
            await expect(hostPage.locator('.room-container')).toBeVisible();

            // Lock with a password in one go, via the toggle.
            await hostPage.click('#settings-btn');
            await hostPage.click('button:has-text("Moderator")');
            await hostPage.fill('#lock-password-input', 'stored');
            await hostPage.click('#lock-room-toggle');
            await expect(hostPage.locator('#apply-password-btn')).toBeDisabled();
            await hostPage.click('#close-settings-btn');

            // Reopen: the client never learns the value, but the server says one
            // exists, so the "nobody can join" warning must not appear.
            await hostPage.click('#settings-btn');
            await hostPage.click('button:has-text("Moderator")');
            await expect(hostPage.locator('.settings-hint').filter({
                hasText: 'Guests need this password to join',
            })).toBeVisible();
            await expect(hostPage.locator('.settings-hint').filter({
                hasText: 'locked without a password',
            })).toBeHidden();
        } finally {
            await hostCtx.close();
        }
    });
});
