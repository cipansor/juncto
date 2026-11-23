import { expect, test } from '@playwright/test';
import { paintedBackdrops, contrastRatio } from './contrast';

/**
 * The contrast helper has to sample the surface behind the words, not the whole
 * element box. These cases pin that down with layouts where the two differ
 * sharply, which is hard to see on the real pages: the landing gradient is
 * nearly flat behind its card, so a wrong sample still lands on the right
 * colour and nothing fails.
 */
test.describe('contrast helper sampling', () => {
    test('samples the text backdrop, not the empty part of the box', async ({ page }) => {
        // A wide paragraph whose text sits in a white band on the left while the
        // rest of the box is black. Sampling the full box returns black; the
        // glyphs actually sit on white.
        await page.setContent(`
            <div style="width:600px;height:60px;background:linear-gradient(to right,#ffffff 0 25%,#000000 25% 100%)">
              <p id="t" style="margin:0;padding:0;width:100%;font:16px sans-serif;color:#ffffff">hi</p>
            </div>
        `);
        const surfaces = await paintedBackdrops(page, '#t');
        const fg: [number, number, number] = [255, 255, 255];
        const worst = Math.min(...surfaces.map((s) => contrastRatio(fg, s)));
        // White on white: the text is invisible where it actually is.
        expect(worst, `surfaces: ${JSON.stringify(surfaces)}`).toBeLessThan(1.5);
    });

    test('reports one surface per line when a gradient crosses the text', async ({ page }) => {
        // Two lines on a vertical gradient: the first is dark, the second light.
        await page.setContent(`
            <div style="width:300px;height:60px;background:linear-gradient(to bottom,#000000 0 50%,#ffffff 50% 100%)">
              <p id="t" style="margin:0;padding:0;font:16px/30px sans-serif;color:#888888">first line<br>second line</p>
            </div>
        `);
        const surfaces = await paintedBackdrops(page, '#t');
        expect(surfaces.length).toBeGreaterThanOrEqual(2);
        const luminances = surfaces.map((s) => s[0] + s[1] + s[2]);
        expect(Math.max(...luminances) - Math.min(...luminances)).toBeGreaterThan(200);
    });

    test('measures each text run against its own colour in a mixed container', async ({ page }) => {
        // A container whose children paint different colours: a white heading on
        // the dark card, and white text on a blue button. Reading the colour from
        // the container would compare the heading's white with the button's blue
        // backdrop and report a contrast failure that no user can see.
        await page.setContent(`
            <div id="box" style="background:#1e293b;padding:16px;width:320px">
              <h3 style="margin:0;color:#f8fafc;font:16px sans-serif">Heading</h3>
              <button id="btn" style="margin-top:12px;padding:8px 16px;border:0;border-radius:6px;background:#3b82f6;color:#ffffff;font:14px sans-serif">Send</button>
            </div>
        `);
        const heading = await paintedBackdrops(page, '#box h3', [248, 250, 252]);
        const button = await paintedBackdrops(page, '#box #btn', [255, 255, 255]);
        // The heading sits on the dark card, the button label on its own blue.
        expect(contrastRatio([248, 250, 252], heading[0])).toBeGreaterThan(10);
        expect(button[0][2], `button backdrop ${button[0]}`).toBeGreaterThan(150);
        expect(contrastRatio([255, 255, 255], button[0])).toBeGreaterThanOrEqual(3);
    });

    test('a readable element passes and an unreadable one fails', async ({ page }) => {
        await page.setContent(`
            <div style="background:#1e293b;padding:20px">
              <p id="ok" style="margin:0;color:#fca5a5;font:12px sans-serif">readable</p>
              <p id="bad" style="margin:0;color:#ef4444;font:12px sans-serif">unreadable</p>
            </div>
        `);
        const ok = await paintedBackdrops(page, '#ok');
        const bad = await paintedBackdrops(page, '#bad');
        expect(contrastRatio([252, 165, 165], ok[0])).toBeGreaterThanOrEqual(4.5);
        expect(contrastRatio([239, 68, 68], bad[0])).toBeLessThan(4.5);
    });
});
