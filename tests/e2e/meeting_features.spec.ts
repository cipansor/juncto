import { test, expect } from '@playwright/test';

test.describe('Meeting Features', () => {
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

  test('Hand Raise and Screen Share Indicators', async ({ browser }) => {
    const roomName = `FeatRoom1_${Date.now()}`;
    const contextA = await browser.newContext();
    const pageA = await contextA.newPage();
    await pageA.goto('/');

    // User A (Host)
    await pageA.fill('input[type="text"]', roomName);
    await pageA.click('button:has-text("Start Meeting")');
    await pageA.locator('.prejoin-container input[type="text"]').fill('Alice');
    await pageA.click('button:has-text("Join Meeting")');
    await expect(pageA.locator('.video-grid')).toBeVisible();

    // User B
    const contextB = await browser.newContext();
    const pageB = await contextB.newPage();
    await pageB.goto(`/room/${roomName}`);
    await pageB.locator('.prejoin-container input[type="text"]').fill('Bob');
    await pageB.click('button:has-text("Join Meeting")');
    await expect(pageB.locator('.video-grid')).toBeVisible();

    // 1. Raise Hand
    // Bob clicks Raise Hand
    await pageB.click('button:has-text("Raise Hand")');

    // Alice should see Bob's hand raised in list
    await expect(pageA.locator('li:has-text("Bob")')).toContainText('✋');

    // Alice should see Bob's hand raised in video grid
    // Need to find the video card for Bob.
    // The name tag is inside the video card.
    // We look for a video card that contains text "Bob" and "✋"
    await expect(pageA.locator('.video-card', { hasText: 'Bob' }).locator('text=✋')).toBeVisible();

    // Bob toggles it off; the button relabels itself once the hand is up
    await pageB.click('button:has-text("Lower Hand")');
    await expect(pageA.locator('li:has-text("Bob")')).not.toContainText('✋');
    await expect(pageA.locator('.video-card', { hasText: 'Bob' }).locator('text=✋')).not.toBeVisible();

    // 2. Screen Share
    // Bob clicks Screen Share
    await pageB.click('button:has-text("Share Screen")');
    // Bob should see "My Screen"
    await expect(pageB.locator('.video-card', { hasText: 'My Screen' })).toBeVisible();

    // Alice should see "Bob's Screen"
    // Wait for the new card to appear
    await expect(pageA.locator('.video-card', { hasText: "Bob's Screen" })).toBeVisible({ timeout: 15000 });
    // Video stream simulation takes place, we don't necessarily see the placeholder in e2e
    // await expect(pageA.locator('.video-card', { hasText: "Bob's Screen" }).locator('.screen-placeholder')).toBeVisible({ timeout: 15000 });

    // Bob stops sharing
    await pageB.click('button:has-text("Share Screen")');
    await expect(pageB.locator('.video-card', { hasText: 'My Screen' })).not.toBeVisible();
    await expect(pageA.locator('.video-card', { hasText: "Bob's Screen" })).not.toBeVisible();

    await contextA.close();
    await contextB.close();
  });

  test('Polls Creation and Voting', async ({ browser }) => {
    const roomName = `FeatRoom2_${Date.now()}`;
    const contextA = await browser.newContext();
    const pageA = await contextA.newPage();
    await pageA.goto('/');

    // User A (Host)
    await pageA.fill('input[type="text"]', roomName);
    await pageA.click('button:has-text("Start Meeting")');
    await pageA.locator('.prejoin-container input[type="text"]').fill('Alice');
    await pageA.click('button:has-text("Join Meeting")');

    // User B
    const contextB = await browser.newContext();
    const pageB = await contextB.newPage();
    await pageB.goto(`/room/${roomName}`);
    await pageB.locator('.prejoin-container input[type="text"]').fill('Bob');
    await pageB.click('button:has-text("Join Meeting")');

    // Alice opens Polls
    await pageA.click('button:has-text("Polls")');

    // Create Poll
    // Inputs are cleared, so we find them by order or placeholder if available.
    // The previous code showed labels "Question", "Option 1", "Option 2".
    // We can use getByLabel if labels are associated, otherwise inputs following labels.
    // The code structure was: label "Question", input.
    // Let's use simple CSS selectors for now as they are cleaner in this context if structure is known.
    // "Question" is the first input in the form group.

    // We need to switch tab to "Create Poll" first?
    // In `polls.rs`: `active_tab` defaults to "active". Tabs: "Active Polls", "Create Poll".
    // Use .tabs container to target the tab button specifically
    await pageA.locator('.tabs button', { hasText: 'Create Poll' }).click();

    // Use robust selectors based on labels
    await pageA.locator('.form-group', { hasText: 'Question' }).locator('input').fill('Favorite Color?');
    await pageA.locator('.form-group', { hasText: 'Option 1' }).locator('input').fill('Red');
    await pageA.locator('.form-group', { hasText: 'Option 2' }).locator('input').fill('Blue');

    // Target the submit button specifically (inside tab-content)
    await pageA.locator('.tab-content button', { hasText: 'Create Poll' }).click();

    // Verify Poll appears for Alice in "Active Polls" tab (auto switched?)
    // `polls.rs` switches to "active" on create.
    await expect(pageA.locator('.poll-item', { hasText: 'Favorite Color?' })).toBeVisible();

    // Verify Poll appears for Bob (he needs to open dialog)
    await pageB.click('button:has-text("Polls")');
    await expect(pageB.locator('.poll-item', { hasText: 'Favorite Color?' })).toBeVisible();

    // Bob votes for Red
    await pageB.click('button:has-text("Vote") >> nth=0'); // Vote for first option (Red)

    // Verify vote count updates
    await expect(pageB.locator('li:has-text("Red")')).toContainText('1 votes');
    // Wait for update on Alice's side
    await expect(pageA.locator('li:has-text("Red")')).toContainText('1 votes');

    await contextA.close();
    await contextB.close();
  });

  test('Breakout Rooms', async ({ browser }) => {
    const roomName = `FeatRoom3_${Date.now()}`;
    const contextA = await browser.newContext();
    const pageA = await contextA.newPage();
    await pageA.goto('/');

    // User A (Host)
    await pageA.fill('input[type="text"]', roomName);
    await pageA.click('button:has-text("Start Meeting")');
    await pageA.locator('.prejoin-container input[type="text"]').fill('Alice');
    await pageA.click('button:has-text("Join Meeting")');

    // Create Breakout Room
    await pageA.fill('input[placeholder="New Room Name"]', 'Room Alpha');
    await pageA.click('button:has-text("Create")');

    // Verify room appears
    await expect(pageA.locator('.rooms-list')).toContainText('Room Alpha');

    // Join it
    // The button says "Join".
    await pageA.click('button:has-text("Join")');

    // Verify status changes
    await expect(pageA.locator('h4:has-text("(In Breakout Room)")')).toBeVisible();
    await expect(pageA.locator('button:has-text("Return to Main")')).toBeVisible();

    // Return to main
    await pageA.click('button:has-text("Return to Main")');
    await expect(pageA.locator('h4:has-text("(In Breakout Room)")')).not.toBeVisible();

    await contextA.close();
  });
});
