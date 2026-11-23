import { expect, Page } from '@playwright/test';
import zlib from 'node:zlib';

/** WCAG AA floor for normal-size body text. */
export const AA_TEXT = 4.5;

type Rgb = [number, number, number];

function parseColor(value: string): [number, number, number, number] | null {
    const m = value.match(/rgba?\(([^)]+)\)/);
    if (!m) return null;
    const parts = m[1].split(',').map((p) => parseFloat(p.trim()));
    return [parts[0], parts[1], parts[2], parts[3] ?? 1];
}

function relativeLuminance([r, g, b]: Rgb): number {
    const channel = (c: number) => {
        const s = c / 255;
        return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
    };
    return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

export function contrastRatio(a: Rgb, b: Rgb): number {
    const l1 = relativeLuminance(a);
    const l2 = relativeLuminance(b);
    const [hi, lo] = l1 > l2 ? [l1, l2] : [l2, l1];
    return (hi + 0.05) / (lo + 0.05);
}

/** Minimal PNG reader for the 8-bit RGB/RGBA screenshots Playwright produces. */
function decodePng(buffer: Buffer): { width: number; height: number; pixels: Buffer; channels: number } {
    let pos = 8;
    let width = 0;
    let height = 0;
    let channels = 4;
    const idat: Buffer[] = [];
    while (pos < buffer.length) {
        const length = buffer.readUInt32BE(pos);
        const type = buffer.toString('ascii', pos + 4, pos + 8);
        const data = buffer.subarray(pos + 8, pos + 8 + length);
        if (type === 'IHDR') {
            width = data.readUInt32BE(0);
            height = data.readUInt32BE(4);
            const colorType = data.readUInt8(9);
            channels = { 0: 1, 2: 3, 4: 2, 6: 4 }[colorType] ?? 4;
        } else if (type === 'IDAT') {
            idat.push(data);
        } else if (type === 'IEND') {
            break;
        }
        pos += 12 + length;
    }
    const raw = zlib.inflateSync(Buffer.concat(idat));
    const stride = width * channels;
    const pixels = Buffer.alloc(stride * height);
    let offset = 0;
    for (let y = 0; y < height; y += 1) {
        const filter = raw[offset];
        offset += 1;
        const row = raw.subarray(offset, offset + stride);
        offset += stride;
        const out = pixels.subarray(y * stride, (y + 1) * stride);
        const prev = y > 0 ? pixels.subarray((y - 1) * stride, y * stride) : null;
        for (let x = 0; x < stride; x += 1) {
            const left = x >= channels ? out[x - channels] : 0;
            const up = prev ? prev[x] : 0;
            const upLeft = prev && x >= channels ? prev[x - channels] : 0;
            let value = row[x];
            if (filter === 1) value += left;
            else if (filter === 2) value += up;
            else if (filter === 3) value += (left + up) >> 1;
            else if (filter === 4) {
                const p = left + up - upLeft;
                const pa = Math.abs(p - left);
                const pb = Math.abs(p - up);
                const pc = Math.abs(p - upLeft);
                value += pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft;
            }
            out[x] = value & 255;
        }
    }
    return { width, height, pixels, channels };
}

type TextRun = { color: string; rects: { x: number; y: number; width: number; height: number }[] };

/**
 * Every run of text inside the element, with the colour that run is painted in.
 *
 * Colour is read per text node rather than from the audited element, because an
 * audit target is often a container: `.chat-container` holds a white heading, a
 * muted label and a white-on-blue button. Comparing the container's colour with
 * the backdrop behind the button measures nothing real, so each run is paired
 * with the colour actually used to paint it.
 */
async function textRuns(page: Page, selector: string): Promise<TextRun[]> {
    return page.locator(selector).first().evaluate((el) => {
        const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
        const range = document.createRange();
        const runs: { color: string; rects: { x: number; y: number; width: number; height: number }[] }[] = [];
        for (let node = walker.nextNode(); node; node = walker.nextNode()) {
            if (!node.textContent?.trim()) continue;
            const parent = node.parentElement;
            if (!parent) continue;
            const style = getComputedStyle(parent);
            if (style.visibility === 'hidden' || style.display === 'none') continue;
            if (parseFloat(style.opacity) === 0) continue;
            range.selectNodeContents(node);
            const rects = Array.from(range.getClientRects())
                .filter((r) => r.width > 0 && r.height > 0)
                .map((r) => ({ x: r.x, y: r.y, width: r.width, height: r.height }));
            if (rects.length > 0) runs.push({ color: style.color, rects });
        }
        return runs;
    });
}

/**
 * The surfaces actually behind an element's glyphs, read from a screenshot
 * rather than reconstructed from CSS.
 *
 * Walking `background-color` up the tree misses everything the browser
 * composites: the landing page paints a radial gradient in `background-image`
 * behind a translucent card with `backdrop-filter`, so a reconstructed colour
 * would be the card over the body, not the card over the gradient users see.
 *
 * Sampling the element's whole box is not enough either. A short error message
 * in a wide paragraph leaves most of the strip empty, and over a gradient the
 * commonest colour in that strip need not be the colour behind the words. So
 * the sample is restricted to the text's own line boxes, and the returned list
 * is one surface per line box: a gradient can make the first line and the last
 * line sit on different colours, and both have to pass.
 */
export async function paintedBackdrops(page: Page, selector: string): Promise<Rgb[]> {
    expect(await page.locator(selector).first().boundingBox(), `no layout box for ${selector}`).not.toBeNull();
    const rects = (await textRuns(page, selector)).flatMap((run) => run.rects);
    expect(rects.length, `no text line boxes for ${selector}`).toBeGreaterThan(0);
    const shot = await screenshotWithoutText(page, selector);
    return rects
        .map((rect) => dominantBackdrop(shot, rect))
        .filter((rgb): rgb is Rgb => rgb !== null);
}

type Shot = { width: number; height: number; pixels: Buffer; channels: number };

/**
 * A screenshot of the element with its glyphs made transparent, so every pixel
 * in a line box is the surface behind the words.
 *
 * The alternative — sampling the screenshot with the text still painted and
 * discarding pixels that look like the text colour — cannot be made sound. A
 * backdrop that matches the text colour is exactly the unreadable case, so the
 * filter deletes the evidence: white text on a mostly-white line keeps only the
 * dark patch beside it and scores 21:1 while most of the letters are invisible.
 * Hiding the text removes the guess instead of tuning it.
 */
async function screenshotWithoutText(page: Page, selector: string): Promise<Shot> {
    await page.evaluate((sel) => {
        const el = document.querySelector(sel);
        if (!el) return;
        el.setAttribute('data-contrast-probe', '');
        if (document.getElementById('contrast-probe-style')) return;
        const style = document.createElement('style');
        style.id = 'contrast-probe-style';
        // `-webkit-text-fill-color` leaves the background, borders and shadows
        // alone, which `color: transparent` would not: a `background: currentColor`
        // would vanish and change the very surface being measured.
        style.textContent =
            '[data-contrast-probe], [data-contrast-probe] * { -webkit-text-fill-color: transparent !important; }';
        document.head.appendChild(style);
    }, selector);
    try {
        return decodePng(await page.screenshot());
    } finally {
        await page.evaluate((sel) => {
            document.querySelector(sel)?.removeAttribute('data-contrast-probe');
        }, selector);
    }
}

/**
 * Every backdrop colour under a line box, with how much of the box it covers.
 *
 * Only the visible part of the box is read: a line below the fold is not on the
 * screenshot, and reading past the last row yields `undefined` channel values,
 * which turn into a `NaN` ratio rather than a measurement.
 */
function backdropsForRect(shot: Shot, rect: { x: number; y: number; width: number; height: number }) {
    const { width, height, pixels, channels } = shot;
    // The vertical middle of a line box passes through the glyphs; the padding
    // above and below the text does not.
    const y0 = Math.max(0, Math.floor(rect.y + rect.height * 0.3));
    const y1 = Math.min(Math.floor(rect.y + rect.height * 0.7), Math.floor(rect.y + rect.height), height - 1);
    const x0 = Math.max(0, Math.floor(rect.x));
    const x1 = Math.min(Math.floor(rect.x + rect.width), width - 1);
    if (x1 < x0 || y1 < y0) return [];

    const counts = new Map<string, { count: number; rgb: Rgb }>();
    let total = 0;
    for (let y = y0; y <= y1; y += 1) {
        for (let x = x0; x <= x1; x += 1) {
            const o = (y * width + x) * channels;
            const rgb: Rgb = [pixels[o], pixels[o + 1], pixels[o + 2]];
            const key = rgb.join(',');
            const entry = counts.get(key);
            if (entry) entry.count += 1;
            else counts.set(key, { count: 1, rgb });
            total += 1;
        }
    }
    return [...counts.values()]
        .map((entry) => ({ rgb: entry.rgb, share: entry.count / total }))
        .sort((a, b) => b.share - a.share);
}

/** The colour behind most of the letters on this line, or null if it is offscreen. */
function dominantBackdrop(shot: Shot, rect: { x: number; y: number; width: number; height: number }): Rgb | null {
    return backdropsForRect(shot, rect)[0]?.rgb ?? null;
}

/** Share of a line box a colour must cover before it can hide the text there. */
const SIGNIFICANT_SHARE = 0.1;

/**
 * Assert every run of text inside the element clears the WCAG AA contrast floor
 * against the surface actually painted behind it. The worst run decides: one
 * unreadable line fails the element.
 *
 * Colour is read per run, so a container holding a white heading, a muted label
 * and a dark-on-light button is measured three times with the colour each is
 * painted in. Within a line, every colour covering at least a tenth of the box
 * is scored and the worst wins: a line that is mostly white with a dark patch
 * is unreadable for the letters over the white, even though the patch alone
 * would score well.
 */
export async function expectReadableText(
    page: Page,
    selector: string,
    label: string,
    minRatio = AA_TEXT,
): Promise<number> {
    const runs = await textRuns(page, selector);
    expect(runs.length, `no text inside ${label}`).toBeGreaterThan(0);

    const shot = await screenshotWithoutText(page, selector);

    let worst: { ratio: number; color: string; bg: Rgb } | null = null;
    let measured = 0;
    for (const run of runs) {
        const fg = parseColor(run.color);
        if (!fg) continue;
        const rgb: Rgb = [fg[0], fg[1], fg[2]];
        for (const rect of run.rects) {
            const backdrops = backdropsForRect(shot, rect);
            if (backdrops.length === 0) continue; // Offscreen: nothing to measure.
            measured += 1;
            const scoring = backdrops.filter((b) => b.share >= SIGNIFICANT_SHARE);
            for (const bg of scoring.length > 0 ? scoring : backdrops.slice(0, 1)) {
                const ratio = contrastRatio(rgb, bg.rgb);
                expect(Number.isFinite(ratio), `${label}: non-finite contrast`).toBe(true);
                if (worst === null || ratio < worst.ratio) {
                    worst = { ratio, color: run.color, bg: bg.rgb };
                }
            }
        }
    }
    expect(
        measured,
        `${label}: no visible text line to measure (is it below the fold or clipped?)`,
    ).toBeGreaterThan(0);
    expect(worst, `no measurable text inside ${label}`).not.toBeNull();
    expect(
        worst!.ratio,
        `${label}: contrast ${worst!.ratio.toFixed(2)} (${worst!.color} on rgb(${worst!.bg.join(', ')})) is below ${minRatio}`,
    ).toBeGreaterThanOrEqual(minRatio);
    return worst!.ratio;
}
