import { expect, test } from '@playwright/test';

test.describe('Settings dialog keyboard behaviour', () => {
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

    const joinAsHost = async (page: import('@playwright/test').Page, room: string) => {
        await page.goto('/');
        await page.fill('#meeting-name', room);
        await page.click('.create-btn');
        await page.waitForSelector('#display-name');
        await page.fill('#display-name', 'Alice');
        await page.click('.join-btn');
        await page.waitForSelector('.room-container');
    };

    test('Escape closes the settings dialog', async ({ page }) => {
        await joinAsHost(page, `EscSettings_${Date.now()}`);
        await page.click('#settings-btn');
        await expect(page.locator('#close-settings-btn')).toBeVisible();

        await page.keyboard.press('Escape');
        await expect(page.locator('#close-settings-btn')).toBeHidden();
    });

    test('Escape after navigating away does not panic', async ({ page }) => {
        const errors: string[] = [];
        page.on('pageerror', (e) => errors.push(e.message));

        await joinAsHost(page, `StaleListener_${Date.now()}`);

        // Open the dialog so its window listener is registered, then leave the
        // room page by client-side navigation. The listener used to outlive the
        // room's signals and panic when Escape read the disposed signal.
        await page.click('#settings-btn');
        await expect(page.locator('#close-settings-btn')).toBeVisible();

        await page.goBack();
        await page.waitForSelector('#meeting-name', { timeout: 10000 });

        await page.keyboard.press('Escape');
        await page.waitForTimeout(300);

        expect(errors, `page errors: ${errors.join(' | ')}`).toEqual([]);
    });
});
