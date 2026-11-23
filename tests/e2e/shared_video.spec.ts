import { test, expect } from '@playwright/test';
import { expectReadableText } from './contrast';

test.describe('Shared Video', () => {
    test.beforeEach(async ({ request }) => {
        // Reset room state
        const response = await request.post('/api/rooms', {
            data: {
                room_name: 'Test Room',
                is_locked: false,
                is_recording: false,
                is_lobby_enabled: false,
                max_participants: 100,
                host_id: null,
                e2ee_enabled: false
            }
        });
        expect(response.ok()).toBeTruthy();
        expect(response.status()).toBe(201);
    });

    test('Host can share a video', async ({ browser }) => {
        const roomName = `VideoRoom_${Date.now()}`;
        const contextA = await browser.newContext();
        const pageA = await contextA.newPage();
        await pageA.goto('/');

        // User A (Host)
        await pageA.fill('#meeting-name', roomName);
        await pageA.click('.create-btn');
        await pageA.waitForSelector('#display-name');
        await pageA.fill('#display-name', 'Alice');
        await pageA.click('.join-btn');
        await pageA.waitForSelector('.room-container');

        // Click Share Video
        await pageA.click('button:has-text("Video")');

        // Verify Modal appears and submit
        await expect(pageA.locator('h3:has-text("Share Video")')).toBeVisible();
        await pageA.locator('input[placeholder*="youtube.com"]').fill('https://www.youtube.com/watch?v=dQw4w9WgXcQ');
        // Click Share inside the modal
        await pageA.locator('#submit-shared-video-btn').click();

        // Verify Video Card appears
        await expect(pageA.locator('.shared-video')).toBeVisible();
        await expect(pageA.locator('iframe[src*="dQw4w9WgXcQ"]')).toBeVisible();

        // User B joins
        const contextB = await browser.newContext();
        const pageB = await contextB.newPage();
        await pageB.goto(`/room/${roomName}`);
        await pageB.waitForSelector('#display-name');
        await pageB.fill('#display-name', 'Bob');
        await pageB.click('.join-btn');
        await pageB.waitForSelector('.room-container');

        // Verify Video Card appears for Bob
        await expect(pageB.locator('.shared-video')).toBeVisible();
        await expect(pageB.locator('iframe[src*="dQw4w9WgXcQ"]')).toBeVisible();

        // Host Stops Video
        // When sharing video, button text should change to "Stop Video" or similar
        // Let's check what the button says or use ID if available
        await pageA.click('button:has-text("Stop Video")');

        // Verify removed
        await expect(pageA.locator('.shared-video')).not.toBeVisible();
        await expect(pageB.locator('.shared-video')).not.toBeVisible();

        await contextA.close();
        await contextB.close();
    });
});

test.describe('Shared Video validation', () => {
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
        await page.click('button:has-text("Video")');
        await expect(page.locator('h3:has-text("Share Video")')).toBeVisible();
    };

    test('livestream share link is accepted and embedded', async ({ browser }) => {
        const roomName = `LiveRoom_${Date.now()}`;
        const context = await browser.newContext();
        const page = await context.newPage();
        await joinAsHost(page, roomName);

        // What YouTube's Share button produces for a stream.
        await page
            .locator('input[placeholder*="youtube.com"]')
            .fill('https://www.youtube.com/live/abcdefghijk?si=share-token');
        await page.locator('#submit-shared-video-btn').click();

        await expect(page.locator('.shared-video')).toBeVisible();
        await expect(page.locator('iframe[src*="abcdefghijk"]')).toBeVisible();
        await context.close();
    });

    for (const [label, url] of [
        ['placeholder ID', 'https://www.youtube.com/watch?v=ID'],
        ['truncated ID', 'https://www.youtube.com/watch?v=abc'],
        ['non-YouTube link', 'https://vimeo.com/123456789'],
    ] as const) {
        test(`rejects ${label} without closing the dialog`, async ({ browser }) => {
            const roomName = `BadVideo_${Date.now()}`;
            const context = await browser.newContext();
            const page = await context.newPage();
            await joinAsHost(page, roomName);

            await page.locator('input[placeholder*="youtube.com"]').fill(url);
            await page.locator('#submit-shared-video-btn').click();

            // The error is shown and nothing was broadcast.
            await expect(page.locator('#shared-video-error')).toBeVisible();
            // Inside a dialog the paragraph rule outranks `.form-error` unless
            // it excludes it, so the text would not be the danger colour. The
            // ratio matters too: the fill red only reached 3.89:1 on the dialog
            // surface, under the 4.5:1 AA floor for this 12px text.
            await expect(page.locator('#shared-video-error'))
                .toHaveCSS('color', 'rgb(252, 165, 165)');
            await expectReadableText(page, '#shared-video-error', 'shared video error');
            await expect(page.locator('h3:has-text("Share Video")')).toBeVisible();
            await expect(page.locator('.shared-video')).toHaveCount(0);
            await context.close();
        });
    }
});
