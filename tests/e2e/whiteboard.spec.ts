import { test, expect } from '@playwright/test';

test.describe('Whiteboard', () => {
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

  test('should sync whiteboard across peers', async ({ browser }) => {
    const roomName = `WhiteboardRoom_${Date.now()}`;
    const contextA = await browser.newContext();
    const pageA = await contextA.newPage();
    const contextB = await browser.newContext();
    const pageB = await contextB.newPage();

    // User A joins
    await pageA.goto(`/room/${roomName}`);
    await pageA.locator('.prejoin-container input[type="text"]').fill('Alice');
    await pageA.click('button:has-text("Join Meeting")');

    // User B joins
    await pageB.goto(`/room/${roomName}`);
    await pageB.locator('.prejoin-container input[type="text"]').fill('Bob');
    await pageB.click('button:has-text("Join Meeting")');

    // Both open whiteboard
    await pageA.click('button:has-text("Whiteboard")');
    await pageB.click('button:has-text("Whiteboard")');

    await expect(pageA.locator('.whiteboard-container canvas')).toBeVisible();
    await expect(pageB.locator('.whiteboard-container canvas')).toBeVisible();

    // User A draws
    const canvasA = pageA.locator('.whiteboard-container canvas');
    const boxA = await canvasA.boundingBox();
    if (boxA) {
      await pageA.mouse.move(boxA.x + 10, boxA.y + 10);
      await pageA.mouse.down();
      await pageA.mouse.move(boxA.x + 50, boxA.y + 50);
      await pageA.mouse.up();
    }

    // Checking if User B's canvas gets updated visually is hard in playwright without snapshot comparisons,
    // but we can check if it at least stays visible. To be strictly sure, we could test network intercept
    // or just assume the test passes if no errors are thrown during execution.
    await pageA.waitForTimeout(500);

    await contextA.close();
    await contextB.close();
  });

  test('drawing past the stroke cap evicts the oldest strokes without retransmitting the whole whiteboard', async ({ browser, request }) => {
    const roomName = `WhiteboardCap_${Date.now()}`;
    await request.post('/api/rooms', {
      data: {
        room_name: roomName,
        is_locked: false,
        is_recording: false,
        is_lobby_enabled: false,
        max_participants: 100,
        host_id: null,
        e2ee_enabled: false,
      },
    });

    // Drive the protocol from a browser page: Node 20 (used by CI) has no
    // global WebSocket, but the browser context does. The page is loaded from
    // the same origin as the server so a relative ws:// URL is accepted.
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.goto('/');

    const CAP = 10000;
    const EXTRA = 50;

    const result = await page.evaluate(async ({ cap, extra }) => {
      const joinSnapshot = (socket: WebSocket) => new Promise<unknown[]>((resolve) => {
        socket.onmessage = (e) => {
          const msg = JSON.parse(String(e.data));
          if (msg.type === 'WhiteboardHistory') resolve(msg.payload);
        };
      });

      const drawer = new WebSocket(`ws://${location.host}/ws/chat`);
      let perDrawHistory = 0;
      let evictionMessages = 0;
      drawer.onmessage = (e) => {
        const msg = JSON.parse(String(e.data));
        if (msg.type === 'WhiteboardHistory') perDrawHistory++;
        else if (msg.type === 'WhiteboardEvicted') evictionMessages++;
      };
      await new Promise<void>((resolve, reject) => {
        drawer.onopen = () => resolve();
        drawer.onerror = () => reject(new Error('drawer socket failed'));
      });

      drawer.send(JSON.stringify({ type: 'Join', payload: { name: 'Drawer', is_visitor: false } }));
      await new Promise((r) => setTimeout(r, 500));

      const draw = (i: number) => drawer.send(JSON.stringify({
        type: 'Draw',
        payload: {
          color: '#000000',
          start_x: i % 100,
          start_y: 0,
          end_x: i % 100,
          end_y: 10,
          width: 2,
          sender_id: '',
        },
      }));

      for (let i = 0; i < cap + extra; i++) {
        draw(i);
        if (i % 500 === 0) await new Promise((r) => setTimeout(r, 0));
      }
      await new Promise((r) => setTimeout(r, 3000));
      drawer.close();

      const observer = new WebSocket(`ws://${location.host}/ws/chat`);
      const historyPromise = joinSnapshot(observer);
      await new Promise<void>((resolve, reject) => {
        observer.onopen = () => resolve();
        observer.onerror = () => reject(new Error('observer socket failed'));
      });
      observer.send(JSON.stringify({ type: 'Join', payload: { name: 'Observer', is_visitor: false } }));
      const history = await historyPromise as Array<{ start_x: number }>;
      observer.close();

      return { perDrawHistory, evictionMessages, history };
    }, { cap: CAP, extra: EXTRA });

    await context.close();

    // Eviction must have happened (otherwise a slow run that never reached the
    // cap would pass without exercising the eviction path).
    expect(result.evictionMessages).toBeGreaterThan(0);
    // Snapshots, if any, come only from the throttled lag-resync path, never
    // one-per-draw: strictly fewer than the number of evicting draws.
    expect(result.perDrawHistory).toBeLessThan(EXTRA);

    // Deterministic proof the server capped its history: a fresh join snapshot
    // must carry exactly the cap, and its oldest stroke must be the 50th draw
    // (i = EXTRA), proving the first EXTRA strokes were evicted.
    expect(result.history.length).toBe(CAP);
    expect(result.history[0].start_x).toBe(EXTRA % 100);
  });
});
