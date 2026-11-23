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

    test('a password stored by the toggle can be cleared from the field', async ({
        browser,
        request,
    }) => {
        const roomResp = await request.post('http://localhost:3000/api/rooms', {
            data: {
                room_name: 'ClearTogglePwd',
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

            // Lock with a password through the toggle, then clear the field.
            await hostPage.click('#settings-btn');
            await hostPage.click('button:has-text("Moderator")');
            await hostPage.fill('#lock-password-input', 'meeting42');
            await hostPage.click('#lock-room-toggle');
            await expect(hostPage.locator('#apply-password-btn')).toBeDisabled();

            await hostPage.fill('#lock-password-input', '');
            // Regression: the baseline was adopted from the toggle, so an empty
            // draft compared equal to nothing and Apply stayed dead while the
            // server still admitted guests with meeting42.
            await expect(hostPage.locator('#apply-password-btn')).toBeEnabled();
            // Clearing the field removes the password and leaves the room
            // locked with nobody able to join. The prompt must say so instead
            // of calling it an ordinary save.
            await expect(hostPage.locator('#password-unsaved')).toHaveText(
                'Press Apply Password to remove the room password.',
            );
            await hostPage.click('#apply-password-btn');
            await hostPage.click('#close-settings-btn');

            // The password is gone, so the room is now hard-locked.
            const guestPage = await guestCtx.newPage();
            await guestPage.goto(roomUrl);
            await guestPage.fill('.prejoin-container input[type="text"]', 'Guest');
            await guestPage.click('button.join-btn');
            await expect(guestPage.locator('.toast-container .toast-error'))
                .toHaveText('Room is locked', { timeout: 10000 });
            await expect(guestPage.locator('#room-password')).toHaveCount(0);
        } finally {
            await hostCtx.close();
            await guestCtx.close();
        }
    });

    test('a password can be reapplied after unlocking and relocking', async ({
        browser,
        request,
    }) => {
        const roomResp = await request.post('http://localhost:3000/api/rooms', {
            data: {
                room_name: 'RelockPwd',
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

            await hostPage.click('#settings-btn');
            await hostPage.click('button:has-text("Moderator")');
            await hostPage.fill('#lock-password-input', 'meeting42');
            await hostPage.click('#lock-room-toggle');
            // The toggle stored the draft, so the client must adopt it as the
            // baseline: nothing is unsaved, and Apply is disabled. Without the
            // adoption the same text looks like a pending change.
            await expect(hostPage.locator('.settings-hint').filter({
                hasText: 'Guests need this password to join',
            })).toBeVisible({ timeout: 10000 });
            await expect(hostPage.locator('#password-unsaved')).toBeHidden();
            await expect(hostPage.locator('#apply-password-btn')).toBeDisabled();

            // Unlock: the server drops the password, so the baseline must too.
            await hostPage.click('#lock-room-toggle');
            // Wait for the round-trip: a re-render would otherwise overwrite the
            // field between clearing it and the next toggle click.
            await expect(hostPage.locator('.settings-hint').filter({
                hasText: 'Set a password before locking',
            })).toBeVisible({ timeout: 10000 });

            await hostPage.fill('#lock-password-input', '');
            // The toggle carries whatever is in the field, so the clear must
            // have landed before the click or the relock silently re-stores the
            // old password.
            await expect(hostPage.locator('#lock-password-input')).toHaveValue('');
            await hostPage.click('#lock-room-toggle');
            // Relocked without a password.
            await expect(hostPage.locator('.settings-hint').filter({
                hasText: 'locked without a password',
            })).toBeVisible({ timeout: 10000 });
            await expect(hostPage.locator('#apply-password-btn')).toBeDisabled();

            // Regression: a stale baseline made this look already-saved, so
            // Apply stayed disabled while the room rejected every join.
            await hostPage.fill('#lock-password-input', 'meeting42');
            await expect(hostPage.locator('#apply-password-btn')).toBeEnabled();
            await hostPage.click('#apply-password-btn');
            await hostPage.click('#close-settings-btn');

            const guestPage = await guestCtx.newPage();
            await guestPage.goto(roomUrl);
            await guestPage.fill('.prejoin-container input[type="text"]', 'Guest');
            await guestPage.click('button.join-btn');
            await expect(guestPage.locator('#room-password')).toBeVisible({ timeout: 10000 });
            await guestPage.fill('#room-password', 'meeting42');
            await guestPage.click('button.join-btn');
            await expect(guestPage.locator('.room-container')).toBeVisible({ timeout: 10000 });
        } finally {
            await hostCtx.close();
            await guestCtx.close();
        }
    });

    test('a password with significant spaces is stored exactly as typed', async ({
        browser,
        request,
    }) => {
        const roomResp = await request.post('http://localhost:3000/api/rooms', {
            data: {
                room_name: 'SpacedPwd',
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

            // Lock first, then apply a password whose spaces are part of it.
            await hostPage.click('#settings-btn');
            await hostPage.click('button:has-text("Moderator")');
            await hostPage.click('#lock-room-toggle');
            await hostPage.fill('#lock-password-input', ' planning42 ');
            await hostPage.click('#apply-password-btn');
            await hostPage.click('#close-settings-btn');

            // Regression: Apply trimmed the value, so the shared password with
            // its spaces was rejected as invalid.
            const guestPage = await guestCtx.newPage();
            await guestPage.goto(roomUrl);
            await guestPage.fill('.prejoin-container input[type="text"]', 'Guest');
            await guestPage.click('button.join-btn');
            await expect(guestPage.locator('#room-password')).toBeVisible({ timeout: 10000 });
            await guestPage.fill('#room-password', ' planning42 ');
            await guestPage.click('button.join-btn');
            await expect(guestPage.locator('.room-container')).toBeVisible({ timeout: 10000 });
        } finally {
            await hostCtx.close();
            await guestCtx.close();
        }
    });
});

test('a password replaced by another host can be restored after a handoff back', async ({
    browser,
    request,
}) => {
    const roomResp = await request.post('http://localhost:3000/api/rooms', {
        data: {
            room_name: 'HandoffPwd',
            is_locked: false,
            is_recording: false,
            is_lobby_enabled: false,
            max_participants: 10,
        },
    });
    const roomName = (await roomResp.json()).config.room_name;
    const roomUrl = `/room/${encodeURIComponent(roomName)}`;

    const aliceCtx = await browser.newContext();
    const bobCtx = await browser.newContext();
    const guestCtx = await browser.newContext();

    const openModeratorTab = async (page: import('@playwright/test').Page) => {
        await page.click('#settings-btn');
        await page.click('button:has-text("Moderator")');
    };

    const transferHostTo = async (
        page: import('@playwright/test').Page,
        name: string,
    ) => {
        if (
            (await page.locator('#participants-panel').getAttribute('class'))?.includes(
                'panel-hidden',
            )
        ) {
            await page.click('#toggle-participants-btn');
        }
        const row = page.locator('.participant-item').filter({ hasText: name });
        await row.locator('button[title="Transfer Host"]').click();
    };

    try {
        // Alice hosts and locks the room with a password.
        const alicePage = await aliceCtx.newPage();
        await alicePage.goto(roomUrl);
        await alicePage.fill('.prejoin-container input[type="text"]', 'Alice');
        await alicePage.click('button.join-btn');
        await expect(alicePage.locator('.room-container')).toBeVisible();

        await openModeratorTab(alicePage);
        await alicePage.fill('#lock-password-input', 'planning42');
        await alicePage.click('#lock-room-toggle');
        await expect(
            alicePage.locator('.settings-hint').filter({
                hasText: 'Guests need this password to join',
            }),
        ).toBeVisible({ timeout: 10000 });
        await alicePage.click('#close-settings-btn');

        // Bob joins with the password.
        const bobPage = await bobCtx.newPage();
        await bobPage.goto(roomUrl);
        await bobPage.fill('.prejoin-container input[type="text"]', 'Bob');
        await bobPage.click('button.join-btn');
        await expect(bobPage.locator('#room-password')).toBeVisible({ timeout: 10000 });
        await bobPage.fill('#room-password', 'planning42');
        await bobPage.click('button.join-btn');
        await expect(bobPage.locator('.room-container')).toBeVisible({ timeout: 10000 });
        await expect(alicePage.locator('.participant-item')).toHaveCount(2, {
            timeout: 10000,
        });

        // Hand control to Bob. The room stays locked and stays password-protected.
        await transferHostTo(alicePage, 'Bob');
        await expect(
            alicePage.locator('.settings-hint').filter({ hasText: 'Guests need this password' }),
        ).toHaveCount(0);

        // Bob replaces the password without unlocking.
        await openModeratorTab(bobPage);
        await bobPage.fill('#lock-password-input', 'planning43');
        await expect(bobPage.locator('#apply-password-btn')).toBeEnabled();
        await bobPage.click('#apply-password-btn');
        await bobPage.click('#close-settings-btn');

        // Hand control back to Alice.
        await transferHostTo(bobPage, 'Alice');

        // Regression: Alice's cached baseline still held planning42, so typing
        // it back compared equal and Apply stayed disabled — the room kept
        // rejecting guests with a password the host could not change.
        await openModeratorTab(alicePage);
        await expect(alicePage.locator('#lock-password-input')).toBeVisible({
            timeout: 10000,
        });
        await alicePage.fill('#lock-password-input', 'planning42');
        await expect(alicePage.locator('#apply-password-btn')).toBeEnabled({ timeout: 10000 });
        await alicePage.click('#apply-password-btn');
        await alicePage.click('#close-settings-btn');

        // The restored password is the one that now admits a guest.
        const guestPage = await guestCtx.newPage();
        await guestPage.goto(roomUrl);
        await guestPage.fill('.prejoin-container input[type="text"]', 'Guest');
        await guestPage.click('button.join-btn');
        await expect(guestPage.locator('#room-password')).toBeVisible({ timeout: 10000 });
        await guestPage.fill('#room-password', 'planning42');
        await guestPage.click('button.join-btn');
        await expect(guestPage.locator('.room-container')).toBeVisible({ timeout: 10000 });
    } finally {
        await aliceCtx.close();
        await bobCtx.close();
        await guestCtx.close();
    }
});
