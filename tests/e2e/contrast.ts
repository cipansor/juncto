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

/**
 * The colour actually painted behind an element, read from a screenshot rather
 * than reconstructed from CSS.
 *
 * Walking `background-color` up the tree misses everything the browser
 * composites: the landing page paints a radial gradient in `background-image`
 * behind a translucent card with `backdrop-filter`, so a reconstructed colour
 * would be the card over the body, not the card over the gradient users see.
 * Sampling rendered pixels has no such blind spot.
 */
export async function paintedBackground(page: Page, selector: string): Promise<Rgb> {
    const box = await page.locator(selector).first().boundingBox();
    expect(box, `no layout box for ${selector}`).not.toBeNull();
    const { width, pixels, channels } = decodePng(await page.screenshot());

    // Sample a strip inside the element but clear of the glyphs: the top of the
    // box is padding, so it is background.
    const x0 = Math.max(0, Math.floor(box!.x) + 2);
    const x1 = Math.min(width - 1, Math.floor(box!.x + box!.width) - 3);
    const y0 = Math.max(0, Math.floor(box!.y) + 2);
    const y1 = Math.max(y0, Math.floor(box!.y) + 10);
    expect(x1, `empty sample band for ${selector}`).toBeGreaterThan(x0);

    const counts = new Map<string, { count: number; rgb: Rgb }>();
    for (let y = y0; y <= y1; y += 1) {
        for (let x = x0; x <= x1; x += 1) {
            const o = (y * width + x) * channels;
            const rgb: Rgb = [pixels[o], pixels[o + 1], pixels[o + 2]];
            const key = rgb.join(',');
            const entry = counts.get(key);
            if (entry) entry.count += 1;
            else counts.set(key, { count: 1, rgb });
        }
    }
    // The most common colour wins; glyph antialiasing is a minority of pixels.
    return [...counts.values()].sort((a, b) => b.count - a.count)[0].rgb;
}

/**
 * Assert an element's text clears the WCAG AA contrast floor against the
 * surface actually painted behind it.
 */
export async function expectReadableText(
    page: Page,
    selector: string,
    label: string,
    minRatio = AA_TEXT,
): Promise<number> {
    const locator = page.locator(selector).first();
    const textColor = await locator.evaluate((el) => getComputedStyle(el).color);
    const bg = await paintedBackground(page, selector);
    const fg = parseColor(textColor);
    expect(fg, `unparseable foreground for ${label}`).not.toBeNull();
    const ratio = contrastRatio([fg![0], fg![1], fg![2]], bg);
    expect(
        ratio,
        `${label}: contrast ${ratio.toFixed(2)} (${textColor} on rgb(${bg.join(', ')})) is below ${minRatio}`,
    ).toBeGreaterThanOrEqual(minRatio);
    return ratio;
}
