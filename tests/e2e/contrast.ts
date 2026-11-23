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
function decodePng(buffer: Buffer): { width: number; pixels: Buffer; channels: number } {
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
    return { width, pixels, channels };
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
export async function paintedBackdrops(
    page: Page,
    selector: string,
    /** The text colour, whose pixels are glyphs and not backdrop. */
    textColor?: Rgb,
): Promise<Rgb[]> {
    const box = await page.locator(selector).first().boundingBox();
    expect(box, `no layout box for ${selector}`).not.toBeNull();
    const runs = await textRuns(page, selector);
    const rects = runs.flatMap((run) => run.rects);
    expect(rects.length, `no text line boxes for ${selector}`).toBeGreaterThan(0);
    return backdropsForRects(page, rects, textColor);
}

/**
 * The surface painted behind each of the given boxes, one per box.
 *
 * Sampling an element's whole box is not enough: a short message in a wide
 * paragraph leaves most of the strip empty, and over a gradient the commonest
 * colour in that strip need not be the colour behind the words. Each line box
 * is sampled on its own so a gradient crossing the text is measured everywhere
 * it is painted, not just where it happens to be most common.
 */
async function backdropsForRects(
    page: Page,
    rects: { x: number; y: number; width: number; height: number }[],
    textColor?: Rgb,
): Promise<Rgb[]> {
    const { width, pixels, channels } = decodePng(await page.screenshot());

    const surfaces: Rgb[] = [];
    for (const rect of rects) {
        // The vertical middle of a line box passes through the glyphs; the
        // padding above and below the text does not.
        const y0 = Math.max(0, Math.floor(rect.y + rect.height * 0.3));
        const y1 = Math.min(Math.floor(rect.y + rect.height * 0.7), rect.y + rect.height);
        const x0 = Math.max(0, Math.floor(rect.x));
        const x1 = Math.min(width - 1, Math.floor(rect.x + rect.width));
        if (x1 <= x0 || y1 <= y0) continue;

        // Glyph pixels are not backdrop. Bold or large text can cover most of
        // its own line box, and then the commonest colour is the text itself,
        // which would report the element as unreadable against its own colour.
        // Skip anything close to the text colour; keep a fallback for text that
        // genuinely sits on a background of the same colour, where every pixel
        // is a glyph pixel and the honest answer is that nothing is readable.
        const isGlyph = (rgb: Rgb) =>
            textColor !== undefined &&
            Math.abs(rgb[0] - textColor[0]) +
                Math.abs(rgb[1] - textColor[1]) +
                Math.abs(rgb[2] - textColor[2]) <
                90;

        const collect = (skipGlyphs: boolean) => {
            const counts = new Map<string, { count: number; rgb: Rgb }>();
            for (let y = y0; y <= y1; y += 1) {
                for (let x = x0; x <= x1; x += 1) {
                    const o = (y * width + x) * channels;
                    const rgb: Rgb = [pixels[o], pixels[o + 1], pixels[o + 2]];
                    if (skipGlyphs && isGlyph(rgb)) continue;
                    const key = rgb.join(',');
                    const entry = counts.get(key);
                    if (entry) entry.count += 1;
                    else counts.set(key, { count: 1, rgb });
                }
            }
            return [...counts.values()].sort((a, b) => b.count - a.count);
        };

        const backdrop = collect(true);
        const ranked = backdrop.length > 0 ? backdrop : collect(false);
        if (ranked.length === 0) continue;
        surfaces.push(ranked[0].rgb);
    }
    expect(surfaces.length, `no sampleable line box`).toBeGreaterThan(0);
    return surfaces;
}

/**
 * Assert every run of text inside the element clears the WCAG AA contrast floor
 * against the surface actually painted behind it. The worst run decides: one
 * unreadable line fails the element.
 */
export async function expectReadableText(
    page: Page,
    selector: string,
    label: string,
    minRatio = AA_TEXT,
): Promise<number> {
    const runs = await textRuns(page, selector);
    expect(runs.length, `no text inside ${label}`).toBeGreaterThan(0);

    let worst: { ratio: number; color: string; bg: Rgb } | null = null;
    for (const run of runs) {
        const fg = parseColor(run.color);
        if (!fg) continue;
        const rgb: Rgb = [fg[0], fg[1], fg[2]];
        for (const bg of await backdropsForRects(page, run.rects, rgb)) {
            const ratio = contrastRatio(rgb, bg);
            if (worst === null || ratio < worst.ratio) {
                worst = { ratio, color: run.color, bg };
            }
        }
    }
    expect(worst, `no measurable text inside ${label}`).not.toBeNull();
    expect(
        worst!.ratio,
        `${label}: contrast ${worst!.ratio.toFixed(2)} (${worst!.color} on rgb(${worst!.bg.join(', ')})) is below ${minRatio}`,
    ).toBeGreaterThanOrEqual(minRatio);
    return worst!.ratio;
}
