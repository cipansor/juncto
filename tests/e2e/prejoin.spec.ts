import { test, expect } from '@playwright/test';

test.describe('Prejoin Screen', () => {
  test.beforeEach(async ({ page, request }) => {
    // Reset backend state
    const response = await request.post('/api/rooms', {
      data: {
        room_name: "Test Room",
        is_locked: false,
        is_recording: false,
        is_lobby_enabled: false,
        max_participants: 100,
        host_id: null,
        e2ee_enabled: false
      }
    });
    expect(response.status()).toBe(201);
    // Go directly to a room to trigger Prejoin
    await page.goto('/room/e2e-test-room');
  });

  test('should show prejoin screen by default', async ({ page }) => {
    await expect(page.locator('h2')).toHaveText('Join Meeting');
    await expect(page.locator('input[type="text"]')).toHaveValue('Guest');
    // When camera is not ready or failing in headless CI, it shows "Camera is Off"
    // Since Playwright headless mode often struggles with fake media devices,
    // we'll check for EITHER the video tag OR the fallback text to avoid flakiness.
    await expect(
        page.locator('video').or(page.locator('.camera-off-text'))
    ).toBeVisible({ timeout: 10000 });
  });

  test('should join with default settings (Camera ON)', async ({ page }) => {
    await page.fill('input[type="text"]', 'Alice');
    await page.click('button:has-text("Join Meeting")');

    // Should navigate to room
    await expect(page.locator('.room-container')).toBeVisible({ timeout: 10000 });

    // Depending on the mocked media stream, it might be 'Camera Off' or a video.
    // We check the participant name to ensure we joined successfully.
    await expect(page.locator('.local-video')).toContainText('Me', { timeout: 10000 });
  });

  test('should join with Camera OFF', async ({ page }) => {
    await page.fill('input[type="text"]', 'Bob');

    // Wait until video or fallback is visible so we know the state is settled
    await expect(
        page.locator('video').or(page.locator('.camera-off-text'))
    ).toBeVisible({ timeout: 10000 });

    // Ensure we are toggling to OFF by checking the button text
    const camBtn = page.locator('button[title="Toggle Camera"]');
    await expect(camBtn).toBeVisible();
    if (await camBtn.textContent() === '📷') {
        await camBtn.click(); // Toggle it off
    }

    // Verify preview shows "Camera is Off" text or similar fallback
    // In PrejoinScreen fallback: <div class="camera-off-text" style="color: white;">Camera is Off</div>
    await expect(page.locator('.camera-off-text')).toBeVisible({ timeout: 10000 });

    await page.click('button:has-text("Join Meeting")');

    // Should navigate to room
    await expect(page.locator('.room-container')).toBeVisible({ timeout: 10000 });

    // Check local video shows "Camera Off" fallback
    await expect(page.locator('.local-video')).toContainText('Camera Off', { timeout: 10000 });
    await expect(page.locator('.local-video video')).not.toBeVisible();
  });
});

test.describe('Home form validation', () => {
    test('blank room name is rejected instead of creating an unreachable room', async ({ page }) => {
        await page.goto('/');
        await page.fill('#meeting-name', '   ');
        await page.click('.create-btn');

        await expect(page.locator('#meeting-name-error')).toBeVisible();
        // Still on the home page: no navigation into a room was attempted.
        await expect(page.locator('#meeting-name')).toBeVisible();
        await expect(page.locator('.room-container')).toHaveCount(0);
    });

    test('error message uses the shared stylesheet class', async ({ page }) => {
        await page.goto('/');
        await page.fill('#meeting-name', '');
        await page.click('.create-btn');

        const error = page.locator('#meeting-name-error');
        await expect(error).toBeVisible();
        await expect(error).toHaveClass(/form-error/);
        // Colours come from the stylesheet, not inline attributes.
        await expect(error).not.toHaveAttribute('style', /color/);
        // Regression: `.hero-card p` is more specific than `.form-error` alone,
        // so the message rendered in the muted body colour, not the danger one.
        await expect(error).toHaveCSS('color', 'rgb(239, 68, 68)');
    });
});
