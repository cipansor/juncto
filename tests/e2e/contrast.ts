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
    const { painted, glyphs } = await glyphLayers(page, selector);
    return rects
        .map((rect) => dominantBackdrop(painted, glyphs, rect))
        .filter((rgb): rgb is Rgb => rgb !== null);
}

type Shot = { width: number; height: number; pixels: Buffer; channels: number };

/** How close a pixel must be to the sentinel colour to count as glyph paint. */
const SENTINEL_TOLERANCE = 40;

/**
 * Colours used to paint the text when looking for its footprint. Each is
 * checked against the rendered page first, and the first one that appears
 * nowhere is used.
 */
const SENTINEL_CANDIDATES: Rgb[] = [
    [255, 0, 255],
    [0, 255, 0],
    [0, 255, 255],
    [255, 255, 0],
    [255, 0, 0],
    [0, 0, 255],
];

/**
 * The surface behind the glyphs, plus a mask of which pixels a glyph covers.
 *
 * Two screenshots: one with the text made transparent, which is the true
 * surface, and one with the text painted in a colour that occurs nowhere on the
 * page, which marks the glyph footprint. The mask is then "pixels painted in the
 * sentinel colour", so it does not depend on the text's own colour or on the
 * backdrop.
 *
 * Diffing the two screenshots would be the obvious shortcut and it is wrong: a
 * glyph painted the same colour as its backdrop changes no pixel, so the mask
 * would be blind exactly where the text is invisible — the case the audit exists
 * for. Picking the sentinel as "the colour farthest from the backdrop" fails the
 * same way, because a glyph already painted that colour is a no-op.
 */
async function glyphLayers(page: Page, selector: string): Promise<{ painted: Shot; glyphs: Uint8Array }> {
    const applyFill = async (fill: string | null) => {
        const ok = await page.evaluate(
            ([sel, value]) => {
                const el = document.querySelector(sel as string);
                if (!el) return false;
                const id = 'contrast-probe-style';
                let style = document.getElementById(id) as HTMLStyleElement | null;
                if (!style) {
                    style = document.createElement('style');
                    style.id = id;
                    document.head.appendChild(style);
                }
                if (value === null) {
                    el.removeAttribute('data-contrast-probe');
                    style.textContent = '';
                    return true;
                }
                el.setAttribute('data-contrast-probe', '');
                // `-webkit-text-fill-color` leaves the background, borders and
                // shadows alone, which `color` would not: a `background:
                // currentColor` would vanish and change the very surface being
                // measured.
                style.textContent = `[data-contrast-probe], [data-contrast-probe] * { -webkit-text-fill-color: ${value} !important; }`;
                return true;
            },
            [selector, fill] as [string, string | null],
        );
        return ok;
    };

    try {
        if (!(await applyFill('transparent'))) return { painted: decodePng(await page.screenshot()), glyphs: new Uint8Array(0) };
        const painted = decodePng(await page.screenshot());
        const sentinel = pickSentinel(painted);
        await applyFill(`rgb(${sentinel.join(', ')})`);
        const withText = decodePng(await page.screenshot());

        const glyphs = new Uint8Array(withText.width * withText.height);
        for (let i = 0; i < glyphs.length; i += 1) {
            const o = i * withText.channels;
            const distance =
                Math.abs(withText.pixels[o] - sentinel[0]) +
                Math.abs(withText.pixels[o + 1] - sentinel[1]) +
                Math.abs(withText.pixels[o + 2] - sentinel[2]);
            if (distance <= SENTINEL_TOLERANCE) glyphs[i] = 1;
        }
        return { painted, glyphs };
    } finally {
        await applyFill(null);
    }
}

/** A colour that appears nowhere in the screenshot, so painted text is unambiguous. */
function pickSentinel(shot: Shot): Rgb {
    const present = new Set<string>();
    for (let i = 0; i < shot.width * shot.height; i += 1) {
        const o = i * shot.channels;
        present.add(`${shot.pixels[o]},${shot.pixels[o + 1]},${shot.pixels[o + 2]}`);
    }
    return SENTINEL_CANDIDATES.find((c) => !present.has(c.join(','))) ?? SENTINEL_CANDIDATES[0];
}

/**
 * Every backdrop colour under a line box's glyphs, most common first.
 *
 * Only glyph pixels are counted, so the result does not depend on how much
 * empty space the line box happens to contain. Only rows inside the screenshot
 * are read: a line below the fold has no pixels, and reading past the last row
 * yields `undefined` channels and a `NaN` ratio.
 */
function backdropsForRect(
    painted: Shot,
    glyphs: Uint8Array,
    rect: { x: number; y: number; width: number; height: number },
) {
    const { width, height, pixels, channels } = painted;
    const y0 = Math.max(0, Math.floor(rect.y));
    const y1 = Math.min(Math.floor(rect.y + rect.height), height - 1);
    const x0 = Math.max(0, Math.floor(rect.x));
    const x1 = Math.min(Math.floor(rect.x + rect.width), width - 1);
    if (x1 < x0 || y1 < y0) return [];

    const counts = new Map<string, { count: number; rgb: Rgb }>();
    let total = 0;
    for (let y = y0; y <= y1; y += 1) {
        for (let x = x0; x <= x1; x += 1) {
            const i = y * width + x;
            if (glyphs[i] !== 1) continue;
            const o = i * channels;
            const rgb: Rgb = [pixels[o], pixels[o + 1], pixels[o + 2]];
            const key = rgb.join(',');
            const entry = counts.get(key);
            if (entry) entry.count += 1;
            else counts.set(key, { count: 1, rgb });
            total += 1;
        }
    }
    if (total === 0) return [];
    return [...counts.values()]
        .map((entry) => ({ rgb: entry.rgb, share: entry.count / total }))
        .sort((a, b) => b.share - a.share)
        .map((entry) => entry.rgb);
}

/** The colour behind most of the letters on this line, or null if none are visible. */
function dominantBackdrop(
    painted: Shot,
    glyphs: Uint8Array,
    rect: { x: number; y: number; width: number; height: number },
): Rgb | null {
    return backdropsForRect(painted, glyphs, rect)[0] ?? null;
}

/**
 * Assert every run of text inside the element clears the WCAG AA contrast floor
 * against the surface actually painted behind it. The worst run decides: one
 * unreadable line fails the element.
 *
 * Colour is read per run, so a container holding a white heading, a muted label
 * and a dark-on-light button is measured three times with the colour each is
 * painted in. Within a line, every backdrop under the glyphs is scored and the
 * worst wins: a line whose letters are partly over a light patch is unreadable
 * for those letters, even though the rest of the line scores well.
 */
export async function expectReadableText(
    page: Page,
    selector: string,
    label: string,
    minRatio = AA_TEXT,
): Promise<number> {
    const runs = await textRuns(page, selector);
    expect(runs.length, `no text inside ${label}`).toBeGreaterThan(0);

    const { painted, glyphs } = await glyphLayers(page, selector);

    let worst: { ratio: number; color: string; bg: Rgb } | null = null;
    let measured = 0;
    for (const run of runs) {
        const fg = parseColor(run.color);
        if (!fg) continue;
        const rgb: Rgb = [fg[0], fg[1], fg[2]];
        for (const rect of run.rects) {
            const backdrops = backdropsForRect(painted, glyphs, rect);
            if (backdrops.length === 0) continue; // No visible glyphs on this line.
            measured += 1;
            // Every colour under the letters is scored, with no minimum share.
            // A share threshold is a false-pass by construction: text crossing a
            // narrow unreadable band is exactly the case that matters, and the
            // band is precisely what a threshold discards.
            for (const bg of backdrops) {
                const ratio = contrastRatio(rgb, bg);
                expect(Number.isFinite(ratio), `${label}: non-finite contrast`).toBe(true);
                if (worst === null || ratio < worst.ratio) {
                    worst = { ratio, color: run.color, bg };
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
