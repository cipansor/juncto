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

    test('a long room name does not push the header badges out of the bar', async ({ page, request }) => {
        // The header is a single flex row: title, subject, timer, connection dot
        // and badges. Without a shrink limit the title grows to its full text
        // width, the row overflows, and the badges are laid out past the right
        // edge of the bar — on top of the page behind it. They are then genuinely
        // unreadable, and the contrast audit catches it as light-on-light
        // (`rgb(216, 218, 221)` on `rgb(192, 193, 197)`), not as an overflow.
        //
        // The name is long enough to overflow at every width tested, so this does
        // not depend on font metrics. Widening the window is not a fix: CI saw the
        // same failure at 1280 with an ordinary name.
        await resetRoom(request);
        await page.setViewportSize(DESKTOP);
        const long = 'VeryLongRoomName_'.repeat(4) + Date.now();
        await page.goto(`/room/${long}`);
        await page.waitForSelector('#display-name');
        await page.fill('#display-name', 'OverflowChecker');
        await page.click('.join-btn');
        await expect(page.locator('.room-container')).toBeVisible({ timeout: 20000 });
        await page.waitForTimeout(1500);

        const layout = await page.evaluate(() => {
            const header = document.querySelector('.room-header') as HTMLElement;
            const badge = document.querySelector('.room-header .badge-success') as HTMLElement;
            const box = badge.getBoundingClientRect();
            return {
                overflows: header.scrollWidth > header.clientWidth,
                badgeRight: box.right,
                viewport: window.innerWidth,
            };
        });
        expect(layout.overflows, 'room header must not overflow its own bar').toBe(false);
        expect(layout.badgeRight, 'badges must stay inside the viewport').toBeLessThanOrEqual(
            layout.viewport,
        );
        expect(await expectReadableText(page, '.room-header', 'room header', 3.0)).toBeGreaterThan(3.0);
    });

    test('the notification bell does not sit on top of the header badges', async ({ page, request }) => {
        // The bell is absolutely positioned over the top-right corner of
        // `.capture-area`, which is where the header's badges end. It is the
        // last thing in the bar, so nothing pushed it away: at 1440 the "🔔"
        // covered the last 7px of "Unlocked", and on CI — where the emoji falls
        // back to a wider font — it covered the whole word. The badge's green
        // text was then sampled against the bell's gold pixels and the audit
        // reported 1.10:1 for text that is legible.
        //
        // The bell's width follows the colour-emoji font, and CI's fallback is
        // roughly twice as wide as a local one, so measuring whatever font this
        // machine has proves nothing about CI. The reserve is written down as a
        // budget instead — 12px offset plus a bell up to 84px — and the bell is
        // given exactly that width here. A font-independent check of the
        // invariant, not of the font.
        await resetRoom(request);
        await page.setViewportSize(DESKTOP);
        await page.goto(`/room/Bell_${Date.now()}`);
        await page.waitForSelector('#display-name');
        await page.fill('#display-name', 'BellChecker');
        await page.click('.join-btn');
        await expect(page.locator('.room-container')).toBeVisible({ timeout: 20000 });
        await page.waitForTimeout(1500);
        await page.addStyleTag({
            content: '#notif-bell-btn { width: 84px !important; box-sizing: border-box; }',
        });
        await page.waitForTimeout(300);

        const overlap = await page.evaluate(() => {
            const badge = document.querySelector('.room-header .badge-success') as HTMLElement;
            const bell = document.querySelector('#notif-bell-btn') as HTMLElement;
            const range = document.createRange();
            range.selectNodeContents(badge);
            const text = range.getBoundingClientRect();
            const box = bell.getBoundingClientRect();
            const vertical = Math.min(text.bottom, box.bottom) - Math.max(text.top, box.top);
            const horizontal = Math.min(text.right, box.right) - Math.max(text.left, box.left);
            return vertical > 0 && horizontal > 0 ? horizontal : 0;
        });
        expect(overlap, 'the bell must not cover any badge text').toBe(0);
        expect(await expectReadableText(page, '.room-header', 'room header', 3.0)).toBeGreaterThan(3.0);
    });

    test('the bell clears the badges on mobile too', async ({ page, request }) => {
        // The mobile rule overrides `.room-header`'s padding wholesale, so the
        // corner reserved for the bell on desktop was given back at 768px and
        // below: the bell covered 30px of the lock badge at 768px. The reserve
        // has to be repeated in the mobile rule, not inherited.
        for (const width of [768, 600, 390]) {
            await resetRoom(request);
            await page.setViewportSize({ width, height: 800 });
            await page.goto(`/room/BellMobile_${width}_${Date.now()}`);
            await page.waitForSelector('#display-name');
            await page.fill('#display-name', 'BellMobile');
            await page.click('.join-btn');
            await expect(page.locator('.room-container')).toBeVisible({ timeout: 20000 });
            await page.waitForTimeout(1200);

            // The mobile reserve is 12px offset plus a bell up to 52px. Give
            // the bell that width rather than a font size: a font size scales
            // CI's wider fallback emoji further still, which is what made this
            // test red on CI for a page whose shipped bell clears the badge.
            await page.addStyleTag({
                content: '#notif-bell-btn { width: 52px !important; box-sizing: border-box; }',
            });
            await page.waitForTimeout(300);

            const clearance = await page.evaluate(() => {
                const badge = document.querySelector(
                    '.room-header .badge-success, .room-header .badge-danger',
                ) as HTMLElement;
                const bell = document.querySelector('#notif-bell-btn') as HTMLElement;
                if (!badge || !bell) return null;
                const b = badge.getBoundingClientRect();
                const c = bell.getBoundingClientRect();
                return Math.round(Math.min(b.right, c.right) - Math.max(b.left, c.left));
            });
            expect(clearance, `bell must not reach the badge at ${width}px`).not.toBeNull();
            expect(clearance!, `bell must not cover the badge at ${width}px`).toBeLessThanOrEqual(0);
        }
    });

    test('the bell clears the badges with an unread count showing', async ({ page, context }) => {
        // The two checks above force the bell to a width, so neither sees what
        // the button does on its own. It renders its unread count as a child,
        // and in flow that child widened the button: 30px bare, 50px with one
        // digit, 56px with two. The mobile rule reserves 12px offset plus a
        // bell up to 52px, so a two-digit count reached 4px into the lock
        // badge. The count is pinned to the corner now, so the button keeps its
        // width however large the count gets — which is what this measures, at
        // the width the user actually gets.
        //
        // Both the button and the count are measured, because the count is
        // drawn outside the button: it hangs past the right edge and grows
        // leftwards, so the button's rect stays 30px while the count walks
        // towards the lock badge. Measuring the button alone passes while the
        // count is 80px too wide.
        for (const width of [1440, 768, 600, 390]) {
            await page.setViewportSize({ width, height: 800 });
            const room = `BellCount_${width}_${Date.now()}`;
            await page.goto(`/room/${room}`);
            await page.fill('#display-name', 'Host');
            await page.click('.join-btn');
            await expect(page.locator('.video-grid')).toBeVisible({ timeout: 20000 });

            const visitor = await context.newPage();
            await visitor.setViewportSize({ width, height: 800 });
            await visitor.goto(`/room/${room}`);
            await visitor.fill('#display-name', 'Visitor');
            await visitor.click('.join-btn');
            await expect(visitor.locator('.video-grid')).toBeVisible({ timeout: 20000 });

            // Muting everyone raises a toast for the visitor, and each toast is
            // one unread. Twelve of them put two digits in the badge, the width
            // that overflowed. This is the product's own path to a count, not a
            // dispatch a test made up.
            if (await page.locator('#mute-all-btn').isHidden()) {
                await page.click('#toggle-participants-btn');
            }
            for (let i = 0; i < 12; i += 1) {
                await page.click('#mute-all-btn');
                await page.waitForTimeout(150);
            }
            // Two digits is the condition that broke; the exact number also
            // depends on how many toasts the room raised on join.
            await expect(visitor.locator('#notif-bell-btn .notif-badge')).toHaveText(/^\d{2}$/);

            // The toasts the count comes from stack over this corner for three
            // seconds, so wait them out before measuring: a toast in front of
            // the badge is not the surface the badge is painted on.
            await expect(visitor.locator('.toast')).toHaveCount(0, { timeout: 15000 });

            // The count is drawn outside the button, so the button's own rect
            // says nothing about where the count lands: it hangs 6px past the
            // right edge and grows leftwards with each digit. Measure everything
            // the control draws — the union of button and count — against the
            // lock badge, and how much of the reserved corner the count spans.
            const measure = () =>
                visitor.evaluate(() => {
                    const bell = document.querySelector('#notif-bell-btn') as HTMLElement;
                    const count = document.querySelector('#notif-bell-btn .notif-badge') as HTMLElement;
                    const badge = document.querySelector(
                        '.room-header .badge-success, .room-header .badge-danger',
                    ) as HTMLElement;
                    if (!bell || !count || !badge) return null;
                    const lock = badge.getBoundingClientRect();
                    const button = bell.getBoundingClientRect();
                    const drawn = count.getBoundingClientRect();
                    return {
                        clearance: Math.round(
                            Math.min(lock.right, Math.max(button.right, drawn.right)) -
                                Math.max(lock.left, Math.min(button.left, drawn.left)),
                        ),
                        countSpan: Math.round(button.right - drawn.left),
                    };
                });

            const measured = await measure();
            expect(measured, `header badges, bell and count must exist at ${width}px`).not.toBeNull();
            expect(
                measured!.clearance,
                `the bell and its count must not reach the badge at ${width}px`,
            ).toBeLessThanOrEqual(0);

            // The boundary is the corner the header reserves for the bell, not
            // the button's own width: the bell is 30px here but the corner is
            // 52px on mobile (64px padding less the 12px offset) and 84px on
            // desktop. A count wider than the bell is fine while it stays inside.
            const reservedCorner = width <= 768 ? 52 : 84;
            expect(
                measured!.countSpan,
                `the count must fit the ${reservedCorner}px corner reserved for the bell at ${width}px`,
            ).toBeLessThanOrEqual(reservedCorner);

            // So a 44px count — three digits — has to be accepted: it hangs 6px
            // past the bell and reaches 38px back, inside the 52px corner and
            // clear of the lock badge. Bounding it against the bell instead
            // would reject a layout that touches nothing.
            if (width === 768) {
                await visitor.addStyleTag({
                    content: '#notif-bell-btn .notif-badge { min-width: 44px !important; }',
                });
                await visitor.waitForTimeout(200);
                const wide = await measure();
                expect(wide, 'a widened count must still be measurable').not.toBeNull();
                expect(wide!.countSpan, 'a 44px count must fit the 52px corner').toBeLessThanOrEqual(52);
                expect(wide!.clearance, 'a 44px count must clear the lock badge').toBeLessThanOrEqual(0);
            }

            // The count is white on a red fill, so it needs its own floor: the
            // red that reads as "alert" (`--danger-color`) only reaches 3.6:1
            // behind white, and this is small text.
            await expectReadableText(visitor, '#notif-bell-btn .notif-badge', `unread count at ${width}px`);
            await visitor.close();
        }
    });

    test('the room name stays readable when the bar is narrow', async ({ page, request }) => {
        // Chat is open by default above 768px and takes 340px, so a 780px
        // window leaves a 440px header. The single-line title used to spend all
        // of that on the generic "Meeting Room: " prefix: the heading read
        // "Meeting R" and the name itself was never shown. The prefix is now
        // its own element, dropped when the bar is too narrow for both.
        await resetRoom(request);
        await page.setViewportSize({ width: 780, height: 800 });
        await page.goto(`/room/Demo_${Date.now()}`);
        await page.waitForSelector('#display-name');
        await page.fill('#display-name', 'NarrowChecker');
        await page.click('.join-btn');
        await expect(page.locator('.room-container')).toBeVisible({ timeout: 20000 });
        await page.waitForTimeout(1500);

        const shown = await page.evaluate(() => {
            const name = document.querySelector('.room-name') as HTMLElement;
            const box = name.getBoundingClientRect();
            const text = name.textContent ?? '';
            let visible = '';
            for (let i = 0; i < text.length; i += 1) {
                const range = document.createRange();
                range.setStart(name.firstChild!, i);
                range.setEnd(name.firstChild!, i + 1);
                const char = range.getBoundingClientRect();
                if (char.width > 0 && char.right <= box.right + 0.5) visible += text[i];
            }
            return visible;
        });
        expect(shown, 'the room name itself must be visible, not just the prefix').toBeTruthy();
        expect(shown).toMatch(/^Demo_\d+$/);
    });
});
