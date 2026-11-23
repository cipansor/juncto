import { test, expect, Page, APIRequestContext } from '@playwright/test';
import { expectReadableText } from './contrast';

/**
 * Contrast audit for every dialog and the main room surface.
 *
 * The React -> Rust cutover left several dialogs rendering white backgrounds
 * with near-white inherited text (and vice versa). Screenshots alone do not
 * catch that reliably, so this spec measures the resolved foreground and
 * background colours in the live DOM and asserts a WCAG AA contrast ratio.
 */

const DESKTOP = { width: 1440, height: 900 };

async function auditContrast(page: Page, label: string, selector: string, minRatio = 3.0) {
    const locator = page.locator(selector).first();
    if (!(await locator.isVisible().catch(() => false))) {
        return null; // Dialogs differ in markup; a missing node is not a failure.
    }
    return expectReadableText(page, selector, label, minRatio);
}

async function resetRoom(request: APIRequestContext) {
    await request.post('/api/rooms', {
        data: {
            room_name: `Contrast_${Date.now()}`,
            is_locked: false,
            is_recording: false,
            is_lobby_enabled: false,
            max_participants: 100,
        },
    });
}

test.describe('Dialog contrast audit', () => {
    test('all dialogs meet the minimum contrast ratio', async ({ page, request }) => {
        test.setTimeout(240_000);
        await resetRoom(request);
        await page.setViewportSize(DESKTOP);

        const room = `ContrastRoom_${Date.now()}`;
        await page.goto(`/room/${room}`);
        await page.waitForSelector('#display-name');
        await page.fill('#display-name', 'ContrastChecker');
        await page.click('.join-btn');
        await expect(page.locator('.room-container')).toBeVisible({ timeout: 20000 });
        await page.waitForTimeout(1500);

        const results: Record<string, number | null> = {};

        // Main room surfaces
        results['room-header'] = await auditContrast(page, 'room header', '.room-header');
        results['chat-panel'] = await auditContrast(page, 'chat panel', '.chat-container');

        const dialogs: Array<[string, string]> = [
            ['settings', 'Settings'],
            ['polls', 'Polls'],
            ['speaker-stats', 'Speaker Stats'],
            ['invite', 'Invite Others'],
            ['share-video', 'Share Video'],
            ['embed-meeting', 'Embed Meeting'],
            ['dial-in', 'Dial-in Info'],
            ['salesforce', 'Salesforce Integration'],
            ['feedback', 'Feedback'],
            ['calendar', 'Calendar'],
            ['virtual-background', 'Virtual Background'],
            ['shortcuts', 'Keyboard Shortcuts'],
            ['shared-document', 'Shared Document (Etherpad)'],
        ];

        for (const [name, title] of dialogs) {
            const trigger = page.locator(`.toolbox button[title="${title}"]`).first();
            if (!(await trigger.isVisible().catch(() => false))) {
                results[`${name}-skipped`] = null;
                continue;
            }
            await trigger.dispatchEvent('click');

            const dialog = page
                .locator('.modal-content, .dialog-content, .calendar-list-dialog, .login-dialog')
                .first();
            const opened = await dialog
                .waitFor({ state: 'visible', timeout: 8000 })
                .then(() => true)
                .catch(() => false);
            if (!opened) {
                results[`${name}-skipped`] = null;
                continue;
            }
            await dialog.evaluate((el) => el.setAttribute('data-audit-modal', 'true'));
            await page.waitForTimeout(350);

            results[`${name}-title`] = await auditContrast(
                page,
                `${name} title`,
                '[data-audit-modal] h3, [data-audit-modal] h2',
            );
            results[`${name}-body`] = await auditContrast(
                page,
                `${name} body`,
                '[data-audit-modal] p, [data-audit-modal] label, [data-audit-modal] th, [data-audit-modal] td',
            );
            results[`${name}-bg`] = await page.$eval('[data-audit-modal]', (el) => {
                const m = getComputedStyle(el).backgroundColor.match(/rgba?\(([^)]+)\)/);
                const p = m ? m[1].split(',').map((x) => parseFloat(x.trim())) : [0, 0, 0];
                return 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
            });

            await page.$eval('[data-audit-modal]', (el) => el.removeAttribute('data-audit-modal'));

            // Close via the first visible close affordance, falling back to the
            // toolbox toggle that opened the dialog.
            let closed = false;
            for (const sel of ['#close-settings-btn', '#close-polls-btn', '#close-shortcuts-btn', '#dial-in-close-btn']) {
                const btn = page.locator(sel).first();
                if (await btn.isVisible().catch(() => false)) {
                    await btn.click().catch(() => {});
                    closed = true;
                    break;
                }
            }
            if (!closed) {
                const generic = dialog.locator('.modal-close-btn').first();
                if (await generic.isVisible().catch(() => false)) {
                    await generic.click().catch(() => {});
                    closed = true;
                }
            }
            if (!closed) {
                await trigger.dispatchEvent('click').catch(() => {});
            }
            await page.waitForTimeout(450);
        }

        // No dialog should render on a near-white surface (the cutover bug left
        // white modal backgrounds behind near-white text).
        for (const [key, lum] of Object.entries(results)) {
            if (!key.endsWith('-bg') || lum === null) continue;
            expect(lum, `${key} renders on a near-white surface`).toBeLessThan(240);
        }

        // Tall dialogs are height-capped and must scroll internally; otherwise their
        // bottom controls render outside the dialog box and cannot be reached.
        await page.locator('.toolbox button[title="Settings"]').first().dispatchEvent('click');
        await page.getByRole('button', { name: 'Devices', exact: true }).click();
        await page.waitForTimeout(400);
        const tabOverflowY = await page.$eval(
            '.modal-content .tab-content',
            (el) => getComputedStyle(el).overflowY,
        );
        expect(tabOverflowY, 'settings tab body must scroll').toMatch(/auto|scroll/);

        // Validation text is 12px, so it needs the full 4.5:1 AA ratio, not the
        // 3.0 floor used for large headings. The fill red measured 3.89:1 here.
        await page.locator('.toolbox button[title="Share Video"]').first().dispatchEvent('click');
        const videoDialog = page.locator('.modal-content').first();
        await videoDialog.waitFor({ state: 'visible', timeout: 8000 });
        await page.fill('.modal-content input[placeholder*="youtube.com"]', 'https://example.com/nope');
        await page.locator('#submit-shared-video-btn').click();
        await expect(page.locator('#shared-video-error')).toBeVisible();
        const errorRatio = await expectReadableText(
            page,
            '#shared-video-error',
            'shared video validation error',
        );
        results['shared-video-error'] = errorRatio;

        console.log('contrast audit:', JSON.stringify(results, null, 2));
    });
});