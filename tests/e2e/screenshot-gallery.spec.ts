import { test, expect, Page, APIRequestContext } from '@playwright/test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const OUT = process.env.SCREENSHOT_DIR || '../screenshots';
const OUT_DIR = path.resolve(__dirname, OUT);

/**
 * The gallery is ordered by the numeric prefix of each file name, so a repeated
 * prefix makes the order ambiguous and two views collide in the README tables.
 * Two screens were once both numbered 35.
 *
 * The names are recorded as they are written, not read back from the output
 * directory: a clean checkout has no files to inspect before the run, and two
 * captures that share a name leave a single file behind, so neither an empty
 * directory nor a finished one can prove uniqueness on its own.
 */
function expectUniqueScreenshotNumbers(names: string[]) {
    const seen = new Map<string, string[]>();
    for (const name of names) {
        const number = name.split('-')[0];
        seen.set(number, [...(seen.get(number) ?? []), name]);
    }
    const duplicates = [...seen.entries()].filter(([, group]) => group.length > 1);
    expect(
        duplicates,
        `screenshot numbers must be unique: ${duplicates
            .map(([n, group]) => `${n} used by ${group.join(', ')}`)
            .join('; ')}`,
    ).toEqual([]);
}

/**
 * Every view the gallery must produce. Several captures are conditional (the
 * lobby, the rejoin overlay, the context menu, the mobile settings dialog), and
 * a conditional capture that never happens is simply absent from `captured`, so
 * a uniqueness check alone accepts the run. README.md links all of these, so a
 * missing one is a broken image in the docs.
 */
const REQUIRED_SHOTS = [
    '01-home', '02-prejoin', '03-room', '04-participants', '05-chat',
    '06-settings-profile', '07-settings-devices', '08-settings-integrations',
    '09-settings-more', '10-settings-branding', '11-polls-create',
    '12-polls-active', '13-notifications', '14-whiteboard',
    '15-virtual-background', '16-shortcuts', '17-speaker-stats', '18-invite',
    '19-share-video', '20-embed-meeting', '21-dial-in', '22-salesforce',
    '23-feedback', '24-authentication', '25-calendar', '26-etherpad',
    '27-files', '28-reactions', '29-subtitles', '30-context-menu',
    '31-breakout-rooms', '32-remote-control', '33-room-chat-open',
    '34-visitor-mode', '35-lobby', '36-rejoin-overlay', '37-mobile-home',
    '38-mobile-prejoin', '39-mobile-room', '40-mobile-chat',
    '41-mobile-settings', '42-settings-moderator', '43-room-long-name',
    '44-room-narrow-header',
] as const;

function expectEveryViewCaptured(captured: string[]) {
    const missing = REQUIRED_SHOTS.filter((name) => !captured.includes(name));
    expect(
        missing,
        `the gallery did not capture: ${missing.join(', ')}`,
    ).toEqual([]);
}

const DESKTOP = { width: 1440, height: 900 };
const MOBILE = { width: 420, height: 860 };

/**
 * A control that only exists on one settings tab. `active` on the tab button
 * proves the click registered; this proves the right panel actually rendered,
 * so a capture cannot be filed under a tab whose content never appeared.
 * Devices and Integrations render no ids, hence the text markers.
 */
const PANEL_MARKER: Record<string, string> = {
    Devices: 'text=Camera',
    Integrations: 'text=Salesforce',
    Moderator: '#lock-room-toggle',
    More: '#audio-only-toggle',
    Branding: '#branding-primary-color',
};

async function resetRoom(request: APIRequestContext, overrides: Record<string, unknown> = {}) {
    const response = await request.post('/api/rooms', {
        data: {
            room_name: `Gallery_${Date.now()}`,
            is_locked: false,
            is_recording: false,
            is_lobby_enabled: false,
            max_participants: 100,
            ...overrides,
        },
    });
    expect(response.ok()).toBeTruthy();
}

async function joinRoom(page: Page, name: string, opts: { room: string; visitor?: boolean }) {
    await page.goto(`/room/${encodeURIComponent(opts.room)}`);
    await page.waitForSelector('#display-name');
    await page.fill('#display-name', name);
    if (opts.visitor) await page.check('#visitor-mode');
    await page.click('.join-btn');
    await expect(page.locator('.room-container')).toBeVisible({ timeout: 20000 });
    await page.waitForTimeout(1500);
}

async function openToolbox(page: Page, title: string) {
    await page.locator(`.toolbox button[title="${title}"]`).dispatchEvent('click');
    await page.waitForTimeout(600);
}

async function showPanel(page: Page, toggleId: string, panelSelector: string) {
    const panel = page.locator(`${panelSelector}:not(.panel-hidden)`);
    if (!(await panel.isVisible().catch(() => false))) {
        await page.locator(toggleId).dispatchEvent('click');
        await page.waitForTimeout(500);
    }
    await expect(panel).toBeVisible({ timeout: 10000 });
}

/**
 * Capture one view and remember its name. Every capture goes through here so
 * the uniqueness check sees the declared name even when the write replaces an
 * earlier file, and so a silently skipped capture cannot pass unnoticed.
 */
async function capture(
    target: Page,
    name: string,
    captured: string[],
    dir: string,
) {
    // `captured` is a set — the persisted manifest, so a retry does not re-add
    // the views the first attempt wrote. The append-only list is what the
    // duplicate-number check reads: deduping it would hide a spec that captured
    // the same number twice.
    written.push(name);
    if (!captured.includes(name)) captured.push(name);
    recordCaptured(captured);
    await target.screenshot({ path: `${dir}/${name}.png` });
    expect(
        fs.existsSync(`${dir}/${name}.png`),
        `capture ${name} did not write a file`,
    ).toBe(true);
}

/**
 * The names captured so far, kept on disk as well as in memory.
 *
 * A retry runs in a fresh worker with a fresh module instance, so an in-memory
 * list is empty on the second attempt and `afterAll` then reports the first
 * attempt's views as missing — failing a run whose files are all present. The
 * manifest is scoped to the run id so a stale manifest from an earlier run
 * cannot vouch for a view this run never captured.
 */
const written: string[] = [];

const MANIFEST = path.join(
    os.tmpdir(),
    `juncto-gallery-${process.env.E2E_RUN_ID ?? 'local'}.json`,
);

function loadCaptured(): string[] {
    try {
        const raw = JSON.parse(fs.readFileSync(MANIFEST, 'utf8')) as unknown;
        return Array.isArray(raw) ? raw.filter((n): n is string => typeof n === 'string') : [];
    } catch {
        return [];
    }
}

function recordCaptured(names: string[]) {
    fs.writeFileSync(MANIFEST, JSON.stringify(names));
}

test.describe('UI screenshot gallery', () => {
    const captured: string[] = loadCaptured();

    test('capture every frontend view', async ({ page, context, request }) => {
        test.setTimeout(420_000);
        fs.mkdirSync(OUT_DIR, { recursive: true });
        await resetRoom(request);
        await page.setViewportSize(DESKTOP);

        // 1. Home
        await page.goto('/');
        await page.waitForSelector('#meeting-name');
        await page.waitForTimeout(500);
        await capture(page, '01-home', captured, OUT);

        // 2. Prejoin
        const room = `GalleryRoom_${Date.now()}`;
        await page.goto(`/room/${room}`);
        await page.waitForSelector('#display-name');
        await page.waitForTimeout(1000);
        await capture(page, '02-prejoin', captured, OUT);

        // 3. Room with two participants and chat history
        await page.fill('#display-name', 'Alice');
        await page.click('.join-btn');
        await expect(page.locator('.room-container')).toBeVisible({ timeout: 20000 });
        await page.waitForTimeout(1500);

        const bob = await context.newPage();
        await bob.setViewportSize(DESKTOP);
        await joinRoom(bob, 'Bob', { room });
        await showPanel(bob, '#toggle-chat-btn', '#chat-panel');
        await bob.fill('#chat-input', 'Halo, kita sudah migrasi penuh ke Rust!');
        await bob.keyboard.press('Enter');
        await page.waitForTimeout(500);
        await bob.fill('#chat-input', 'Leptos WASM + Axum dalam satu workspace.');
        await bob.keyboard.press('Enter');
        await page.waitForTimeout(1200);
        await capture(page, '03-room', captured, OUT);

        // 4. Participants panel
        await showPanel(page, '#toggle-participants-btn', '#participants-panel');
        await page.waitForTimeout(700);
        await capture(page, '04-participants', captured, OUT);

        // 5. Chat panel
        await showPanel(page, '#toggle-chat-btn', '#chat-panel');
        await page.waitForTimeout(700);
        await capture(page, '05-chat', captured, OUT);

        // 6-10. Settings tabs
        await openToolbox(page, 'Settings');
        await expect(page.locator('#close-settings-btn')).toBeVisible();
        await page.waitForTimeout(500);
        await capture(page, '06-settings-profile', captured, OUT);

        for (const [tab, shot] of [
            ['Devices', '07-settings-devices'],
            ['Integrations', '08-settings-integrations'],
            ['Moderator', '42-settings-moderator'],
            ['More', '09-settings-more'],
            ['Branding', '10-settings-branding'],
        ] as const) {
            const tabButton = page.locator(`.modal-content .tabs button:has-text("${tab}")`);
            await tabButton.click();
            // The click only marks the tab active once its panel has rendered;
            // without this wait a capture could be filed under the wrong name.
            await expect(tabButton).toHaveClass(/active/);
            await expect(page.locator(PANEL_MARKER[tab]).first()).toBeVisible();
            await page.waitForTimeout(600);
            await capture(page, shot, captured, OUT);
        }
        await page.locator('#close-settings-btn').click();
        await page.waitForTimeout(500);

        // 11-12. Polls
        await openToolbox(page, 'Polls');
        await expect(page.locator('#close-polls-btn')).toBeVisible();
        await page.locator('.modal-tab-btn:has-text("Create Poll")').click();
        await page.fill('#poll-question', 'Framework mana yang dipakai sekarang?');
        await page.fill('#poll-option-1', 'Rust + Leptos');
        await page.fill('#poll-option-2', 'JavaScript lama');
        await page.waitForTimeout(500);
        await capture(page, '11-polls-create', captured, OUT);

        await page.click('#create-poll-submit-btn');
        await page.locator('.modal-tab-btn:has-text("Active Polls")').click();
        await page.waitForTimeout(800);
        await capture(page, '12-polls-active', captured, OUT);
        await page.locator('#close-polls-btn').click();
        await page.waitForTimeout(500);

        // 13. Notifications
        await page.click('#notif-bell-btn');
        await expect(page.locator('#notif-panel')).toBeVisible();
        await page.waitForTimeout(500);
        await capture(page, '13-notifications', captured, OUT);
        await page.click('#notif-bell-btn');
        await page.waitForTimeout(400);

        // 14. Whiteboard
        await openToolbox(page, 'Whiteboard');
        const canvas = page.locator('.whiteboard-container canvas');
        await expect(canvas).toBeVisible({ timeout: 10000 });
        const box = await canvas.boundingBox();
        if (box) {
            await page.mouse.move(box.x + 60, box.y + 60);
            await page.mouse.down();
            await page.mouse.move(box.x + 260, box.y + 150, { steps: 12 });
            await page.mouse.move(box.x + 360, box.y + 70, { steps: 12 });
            await page.mouse.up();
        }
        await page.waitForTimeout(900);
        await capture(page, '14-whiteboard', captured, OUT);
        await page.locator('#toggle-whiteboard-btn').dispatchEvent('click');
        await page.waitForTimeout(500);

        // 15. Virtual background
        await openToolbox(page, 'Virtual Background');
        await expect(page.locator('.modal-content h3')).toContainText('Virtual Background');
        await page.waitForTimeout(500);
        await capture(page, '15-virtual-background', captured, OUT);
        await page.locator('.modal-content .modal-close-btn').first().click();
        await page.waitForTimeout(500);

        // 16. Keyboard shortcuts
        await openToolbox(page, 'Keyboard Shortcuts');
        await expect(page.locator('#close-shortcuts-btn')).toBeVisible();
        await page.waitForTimeout(500);
        await capture(page, '16-shortcuts', captured, OUT);
        await page.locator('#close-shortcuts-btn').click();
        await page.waitForTimeout(500);

        // 17. Speaker stats
        await openToolbox(page, 'Speaker Stats');
        await expect(page.locator('#close-speaker-stats-btn')).toBeVisible();
        await page.waitForTimeout(500);
        await capture(page, '17-speaker-stats', captured, OUT);
        await page.locator('#close-speaker-stats-btn').dispatchEvent('click');
        await page.waitForTimeout(500);

        // 18. Invite
        await openToolbox(page, 'Invite Others');
        await expect(page.locator('.modal-content h3')).toContainText('Invite');
        await page.waitForTimeout(500);
        await capture(page, '18-invite', captured, OUT);
        await page.locator('.modal-content .modal-close-btn').first().click();
        await page.waitForTimeout(500);

        // 19. Share video
        await openToolbox(page, 'Share Video');
        await expect(page.locator('#submit-shared-video-btn')).toBeVisible();
        await page.fill('.modal-content input[type="text"]', 'https://www.youtube.com/watch?v=dQw4w9WgXcQ');
        await page.waitForTimeout(500);
        await capture(page, '19-share-video', captured, OUT);
        await page.locator('#close-shared-video-btn').dispatchEvent('click');
        await page.waitForTimeout(500);

        // 20. Embed meeting
        await openToolbox(page, 'Embed Meeting');
        await expect(page.locator('.dialog-content textarea')).toBeVisible();
        await page.waitForTimeout(500);
        await capture(page, '20-embed-meeting', captured, OUT);
        await page.locator('.dialog-content .modal-close-btn').click();
        await page.waitForTimeout(500);

        // 21. Dial-in
        await openToolbox(page, 'Dial-in Info');
        await expect(page.locator('#dial-in-close-btn')).toBeVisible();
        await page.waitForTimeout(500);
        await capture(page, '21-dial-in', captured, OUT);
        await page.locator('#dial-in-close-btn').dispatchEvent('click');
        await page.waitForTimeout(500);

        // 22. Salesforce
        await openToolbox(page, 'Salesforce Integration');
        await expect(page.locator('#link-salesforce-btn')).toBeVisible();
        await page.waitForTimeout(500);
        await capture(page, '22-salesforce', captured, OUT);
        await page.locator('.modal-content .modal-close-btn').first().click();
        await page.waitForTimeout(500);

        // 23. Feedback
        await openToolbox(page, 'Feedback');
        await expect(page.locator('.submit-feedback-btn')).toBeVisible();
        await page.locator('.feedback-star').nth(4).click();
        await page.waitForTimeout(500);
        await capture(page, '23-feedback', captured, OUT);
        await page.locator('.modal-content .modal-close-btn').first().click();
        await page.waitForTimeout(500);

        // 24. Authentication
        await openToolbox(page, 'Login');
        await expect(page.locator('.login-dialog')).toBeVisible();
        await page.waitForTimeout(500);
        await capture(page, '24-authentication', captured, OUT);
        await page.locator('.login-cancel-btn').click();
        await page.waitForTimeout(500);

        // 25. Calendar
        await openToolbox(page, 'Calendar');
        await expect(page.locator('.calendar-list-dialog')).toBeVisible();
        await page.waitForTimeout(600);
        await capture(page, '25-calendar', captured, OUT);
        await page.locator('.calendar-list-dialog .modal-close-btn').click();
        await page.waitForTimeout(500);

        // 26. Etherpad
        await openToolbox(page, 'Shared Document (Etherpad)');
        await expect(page.locator('.etherpad-container')).toBeVisible({ timeout: 10000 });
        await page.waitForTimeout(800);
        await capture(page, '26-etherpad', captured, OUT);
        await page.locator('#toggle-etherpad-btn').dispatchEvent('click');
        await page.waitForTimeout(500);

        // 27. Files panel
        await showPanel(page, '#toggle-files-btn', '.files-container');
        await page.waitForTimeout(600);
        await capture(page, '27-files', captured, OUT);
        await page.locator('#toggle-files-btn').dispatchEvent('click');
        await page.waitForTimeout(500);

        // 28. Reactions
        await page.locator('.reactions button').first().click();
        await page.waitForTimeout(600);
        await capture(page, '28-reactions', captured, OUT);

        // 29. Subtitles
        await page.locator('#toggle-subtitles-btn').dispatchEvent('click');
        await expect(page.locator('.subtitles-overlay')).toBeVisible({ timeout: 10000 });
        await page.waitForTimeout(700);
        await capture(page, '29-subtitles', captured, OUT);
        await page.locator('#toggle-subtitles-btn').dispatchEvent('click');
        await page.waitForTimeout(500);

        // 30. Video tile context menu
        const remoteCard = page.locator('.video-card:not(.local-video)').first();
        await remoteCard.click({ button: 'right' }).catch(() => {});
        await page.waitForTimeout(700);
        if (await page.locator('.video-context-menu').isVisible().catch(() => false)) {
            await capture(page, '30-context-menu', captured, OUT);
            await page.keyboard.press('Escape');
        } else {
            test.info().annotations.push({ type: 'skip', description: 'context menu not opened' });
        }
        await page.waitForTimeout(500);

        // 31. Breakout rooms
        await page.fill('input[placeholder="New Room Name"]', 'Ruang Diskusi');
        await page.locator('.breakout-rooms button:has-text("Create")').click();
        await expect(page.locator('.rooms-list')).toContainText('Ruang Diskusi');
        await page.waitForTimeout(700);
        await capture(page, '31-breakout-rooms', captured, OUT);

        // 32. Remote control consent dialog (Bob requests control of Alice)
        await showPanel(bob, '#toggle-participants-btn', '#participants-panel');
        const aliceRow = bob.locator('.participants-list li').filter({ hasText: 'Alice' });
        const rcBtn = aliceRow.locator('button[title="Request Remote Control"]');
        await expect(rcBtn.first()).toBeVisible({ timeout: 10000 });
        await rcBtn.first().dispatchEvent('click');
        await expect(page.locator('.modal-content:has-text("Remote Control Request")')).toBeVisible({ timeout: 10000 });
        await page.waitForTimeout(600);
        await capture(page, '32-remote-control', captured, OUT);
        await page.locator('.modal-content button:has-text("Deny")').click();
        await page.waitForTimeout(500);

        // 33. Room with the chat panel open (main layout reference)
        await showPanel(page, '#toggle-chat-btn', '#chat-panel');
        await page.waitForTimeout(600);
        await capture(page, '33-room-chat-open', captured, OUT);

        await bob.close();

        // 34. Visitor (read-only) room
        const visitor = await context.newPage();
        await visitor.setViewportSize(DESKTOP);
        await joinRoom(visitor, 'Tamu Undangan', { room, visitor: true });
        await showPanel(visitor, '#toggle-chat-btn', '#chat-panel');
        await visitor.waitForTimeout(900);
        await capture(visitor, '34-visitor-mode', captured, OUT);
        await visitor.close();

        // 35. Lobby (waiting room). A host must already be present for the
        // lobby to engage, otherwise the backend lets everyone straight in.
        const lobbyRoom = `LobbyRoom_${Date.now()}`;
        await resetRoom(request, { room_name: lobbyRoom, is_lobby_enabled: true });
        const lobbyHost = await context.newPage();
        await lobbyHost.setViewportSize(DESKTOP);
        await joinRoom(lobbyHost, 'Host', { room: lobbyRoom });

        const lobbyPage = await context.newPage();
        await lobbyPage.setViewportSize(DESKTOP);
        await lobbyPage.goto(`/room/${lobbyRoom}`);
        await lobbyPage.waitForSelector('#display-name');
        await lobbyPage.fill('#display-name', 'Menunggu');
        await lobbyPage.click('.join-btn');
        const inLobby = await lobbyPage
            .locator('.lobby-container')
            .waitFor({ state: 'visible', timeout: 20000 })
            .then(() => true)
            .catch(() => false);
        if (inLobby) {
            await lobbyPage.waitForTimeout(1000);
            await capture(lobbyPage, '35-lobby', captured, OUT);
        } else {
            test.info().annotations.push({ type: 'skip', description: 'lobby not entered' });
        }
        await lobbyPage.close();
        await lobbyHost.close();

        // 36. Rejoin overlay after the WebSocket drops. Capture WebSocket
        // instances via an init script so the test can force a disconnect
        // (Playwright's setOffline does not always tear down an open socket).
        const rejoinRoom = `RejoinRoom_${Date.now()}`;
        await resetRoom(request, { room_name: rejoinRoom });
        const rejoinPage = await context.newPage();
        await rejoinPage.setViewportSize(DESKTOP);
        await rejoinPage.addInitScript(() => {
            const Orig = window.WebSocket;
            (window as unknown as { __sockets: WebSocket[] }).__sockets = [];
            window.WebSocket = new Proxy(Orig, {
                construct(target, args) {
                    const ws = new (target as typeof WebSocket)(...args);
                    (window as unknown as { __sockets: WebSocket[] }).__sockets.push(ws);
                    return ws;
                },
            }) as typeof WebSocket;
        });
        await joinRoom(rejoinPage, 'Terputus', { room: rejoinRoom });
        await rejoinPage.evaluate(() => {
            const sockets = (window as unknown as { __sockets?: WebSocket[] }).__sockets ?? [];
            sockets.forEach((s) => s.close());
        });
        const rejoinShown = await rejoinPage
            .locator('.rejoin-overlay')
            .waitFor({ state: 'visible', timeout: 15000 })
            .then(() => true)
            .catch(() => false);
        if (rejoinShown) {
            await rejoinPage.waitForTimeout(600);
            await capture(rejoinPage, '36-rejoin-overlay', captured, OUT);
        } else {
            test.info().annotations.push({ type: 'skip', description: 'rejoin overlay not shown' });
        }
        await rejoinPage.close();

        // 37-41. Mobile viewport
        const mobileRoom = `MobileRoom_${Date.now()}`;
        await resetRoom(request, { room_name: mobileRoom });

        await page.setViewportSize(MOBILE);
        await page.goto('/');
        await page.waitForSelector('#meeting-name');
        await page.waitForTimeout(700);
        await capture(page, '37-mobile-home', captured, OUT);

        await page.goto(`/room/${mobileRoom}`);
        await page.waitForSelector('#display-name');
        await page.waitForTimeout(900);
        await capture(page, '38-mobile-prejoin', captured, OUT);

        await page.fill('#display-name', 'Mobile');
        await page.click('.join-btn');
        await expect(page.locator('.room-container')).toBeVisible({ timeout: 20000 });
        await page.waitForTimeout(1500);
        await capture(page, '39-mobile-room', captured, OUT);

        await page.locator('#toggle-chat-btn').dispatchEvent('click');
        await page.waitForTimeout(900);
        await capture(page, '40-mobile-chat', captured, OUT);

        await openToolbox(page, 'Settings');
        if (await page.locator('#close-settings-btn').isVisible().catch(() => false)) {
            await page.waitForTimeout(700);
            await capture(page, '41-mobile-settings', captured, OUT);
        } else {
            test.info().annotations.push({ type: 'skip', description: 'mobile settings not opened' });
        }
    });

    test('capture the header with a long room name', async ({ page, request }) => {
        // The header is one flex row, and a long room name used to push the
        // badges past its right edge onto the page behind them. The title now
        // truncates, so the bar stays intact whatever the name length.
        fs.mkdirSync(OUT_DIR, { recursive: true });
        await resetRoom(request);
        await page.setViewportSize(DESKTOP);
        const long = 'VeryLongRoomName_'.repeat(4) + Date.now();
        await page.goto(`/room/${long}`);
        await page.waitForSelector('#display-name');
        await page.fill('#display-name', 'GalleryLongName');
        await page.click('.join-btn');
        await expect(page.locator('.room-container')).toBeVisible({ timeout: 20000 });
        await page.waitForTimeout(1500);
        await capture(page, '43-room-long-name', captured, OUT);
    });

    test('capture the header when the bar is narrow', async ({ page, request }) => {
        // Chat is open by default above 768px and takes 340px, so a 780px
        // window leaves a 440px header. The generic prefix used to fill it and
        // the room name was cut off entirely; now the prefix steps aside.
        fs.mkdirSync(OUT_DIR, { recursive: true });
        await resetRoom(request);
        await page.setViewportSize({ width: 780, height: 800 });
        await page.goto(`/room/Narrow_${Date.now()}`);
        await page.waitForSelector('#display-name');
        await page.fill('#display-name', 'GalleryNarrow');
        await page.click('.join-btn');
        await expect(page.locator('.room-container')).toBeVisible({ timeout: 20000 });
        await page.waitForTimeout(1500);
        await capture(page, '44-room-narrow-header', captured, OUT);
    });

    test.afterAll(() => {
        // Runs even when a capture fails partway, so a run that writes the same
        // number twice, or skips a view entirely, is reported rather than left
        // for a reviewer to spot.
        expectUniqueScreenshotNumbers(written);
        expectEveryViewCaptured(captured);
    });
});

