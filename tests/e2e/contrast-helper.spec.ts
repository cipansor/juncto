import { expect, test } from '@playwright/test';
import { paintedBackdrops, contrastRatio, expectReadableText } from './contrast';

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

    test('audits a container with each run own colour, not one colour for all', async ({ page }) => {
        // A container whose children paint different colours: a light heading on
        // the dark card, and white text on a blue button. Every child is legible.
        // Reading one colour from the container and applying it to both children
        // is what this guards: the container's inherited colour (black) scored
        // against the dark card is 1.44:1, a failure no user can see.
        await page.setContent(`
            <div id="box" style="background:#1e293b;padding:16px;width:320px">
              <h3 style="margin:0;color:#f8fafc;font:16px sans-serif">Heading</h3>
              <button id="btn" style="margin-top:12px;padding:8px 16px;border:0;border-radius:6px;background:#1d4ed8;color:#ffffff;font:14px sans-serif">Send</button>
            </div>
        `);
        const ratio = await expectReadableText(page, '#box', 'mixed container', 4.5);
        // Both children pass, and the worst of them is the button (6.70:1).
        expect(ratio).toBeGreaterThanOrEqual(4.5);
        expect(ratio).toBeLessThan(8);
    });

    test('a container fails when one child is unreadable', async ({ page }) => {
        // Same shape, but the button text is dark blue on blue: 1.4:1. The
        // heading alone would pass, so auditing the container has to surface the
        // child that does not. Reusing the heading's colour here would pass.
        await page.setContent(`
            <div id="box" style="background:#1e293b;padding:16px;width:320px">
              <h3 style="margin:0;color:#f8fafc;font:16px sans-serif">Heading</h3>
              <button id="btn" style="margin-top:12px;padding:8px 16px;border:0;border-radius:6px;background:#1d4ed8;color:#1e3a8a;font:14px sans-serif">Send</button>
            </div>
        `);
        await expect(expectReadableText(page, '#box', 'mixed container', 4.5)).rejects.toThrow(
            /contrast .* is below 4\.5/,
        );
    });

    test('scores the letters that sit on the unreadable part of a line', async ({ page }) => {
        // Half the line is black, half is white, and the text is white: every
        // letter over the white half is invisible. The split is painted on the
        // text element itself so it stays at 50% however wide the font renders.
        // Anchoring it to a separate fixed-width patch instead makes the verdict
        // depend on font metrics: the same fixture rejected at letter-spacing
        // 0px and passed at -2px, because the narrower run pushed most of the
        // letters over the black half. That is a test of the font, not of the
        // helper.
        await page.setContent(`
            <p id="t" style="margin:0;width:300px;font:20px/40px sans-serif;color:#ffffff;
               background:linear-gradient(to right,#000000 0 50%,#ffffff 50% 100%)">invisible words over a split</p>
        `);
        await expect(expectReadableText(page, '#t', 'white on white', 3)).rejects.toThrow(
            /contrast 1\.00 .* is below 3/,
        );
    });

    test('a narrow unreadable band is not discarded for being small', async ({ page }) => {
        // A white band only 2% wide crosses the text. It covers 4.3% of the
        // glyph pixels, which is under any sensible "significant share"
        // threshold, so a thresholded audit would score only the black and
        // report 21:1. The letters over the band are still invisible, so the
        // line has to fail. This is why no minimum share is applied.
        await page.setContent(`
            <p id="t" style="margin:0;width:600px;font:20px/40px sans-serif;color:#ffffff;
               background:linear-gradient(to right,#000000 0 40%,#ffffff 40% 42%,#000000 42% 100%)">wide wide wide wide wide wide wide words here now</p>
        `);
        await expect(expectReadableText(page, '#t', 'white on white', 3)).rejects.toThrow(
            /contrast 1\.00 .* is below 3/,
        );
    });

    test('a line below the fold is skipped instead of producing a NaN score', async ({ page }) => {
        // The line box is off the screenshot. Reading rows the PNG does not have
        // yields undefined channel values, and a NaN ratio passes every
        // `toBeGreaterThanOrEqual` check, so an unmeasured line would silently
        // count as readable.
        await page.setViewportSize({ width: 800, height: 300 });
        await page.setContent(`
            <div style="height:2000px"></div>
            <p id="off" style="color:#ffffff;background:#000000;font:16px sans-serif">below the fold</p>
        `);
        expect(await paintedBackdrops(page, '#off')).toEqual([]);
        await expect(expectReadableText(page, '#off', 'offscreen', 3)).rejects.toThrow(
            /no visible text line to measure/,
        );
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
