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

export type Rect = { x: number; y: number; width: number; height: number };

type TextRun = { color: string; rects: Rect[] };

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
        type Run = { color: string; rects: { x: number; y: number; width: number; height: number }[] };
        const runs: Run[] = [];

        // `getClientRects` reports where the text *would* be laid out, not where
        // it can be seen. A title clipped with `overflow: hidden` still reports
        // its full unwrapped width: the room header's `<h2>` clipped at 836px
        // but reported a 1202px text rect, so the clipped-away part was sampled
        // and paired with whatever lay behind the element (white on the pale
        // page, 2.45:1). Intersect with every clipping ancestor in turn.
        const visible = (r: { x: number; y: number; width: number; height: number }, from: Element | null) => {
            let { x, y, width, height } = r;
            for (let el: Element | null = from; el; el = el.parentElement) {
                const cs = getComputedStyle(el);
                if (cs.overflow === 'visible' && cs.overflowX === 'visible' && cs.overflowY === 'visible') continue;
                const box = el.getBoundingClientRect();
                const right = Math.min(x + width, box.right);
                const bottom = Math.min(y + height, box.bottom);
                x = Math.max(x, box.left);
                y = Math.max(y, box.top);
                width = right - x;
                height = bottom - y;
                if (width <= 0 || height <= 0) return null;
            }
            return { x, y, width, height };
        };

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
                .map((r) => ({ x: r.x, y: r.y, width: r.width, height: r.height }))
                .map((r) => visible(r, parent))
                .filter((r): r is { x: number; y: number; width: number; height: number } => r !== null);
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
    const { painted, glyphs, runs } = await glyphLayers(page, selector);
    const rects = runs.flatMap((run) => run.rects);
    expect(rects.length, `no text line boxes for ${selector}`).toBeGreaterThan(0);
    return rects
        .map((rect) => dominantBackdrop(painted, glyphs, rect))
        .filter((rgb): rgb is Rgb => rgb !== null);
}

export type Shot = { width: number; height: number; pixels: Buffer; channels: number };

/**
 * Per-channel difference above which a pixel changed between the two probe
 * fills, i.e. is painted by a glyph.
 */
const GLYPH_DELTA = 24;

/** Two fills far apart from each other; neither has to be absent from the page. */
const PROBE_FILLS: [Rgb, Rgb] = [
    [255, 0, 255],
    [0, 255, 0],
];

/**
 * The colour a run is painted in, measured from the rendered pixels.
 *
 * `color` alone is not the answer: `opacity` multiplies a whole subtree, so a
 * faded panel and the text inside it fade together and the text keeps its
 * declared colour relative to that panel. Black text on a half-opacity white
 * panel over black renders at 5.32:1, and a formula that fades the text without
 * fading the panel reports 2.63:1 — a false failure on readable text.
 *
 * A glyph's edge is antialiased toward whatever surface is behind it, so most
 * glyph pixels are a blend of ink and paper. The ink is therefore taken from the
 * pixels that differ most from their *own* surface: those have the highest
 * coverage. Averaging all glyph pixels, or taking the darkest and lightest, both
 * let a partly covered edge stand in for the ink — on the room header that
 * reported the green badge as `rgb(9, 111, 77)` against `rgb(9, 28, 37)` where
 * the ink is `rgb(16, 185, 129)`.
 *
 * The ink is measured per surface, from the glyph pixels that sit on that
 * surface. One ink for the whole run is a false pass the moment the text is
 * translucent: half-opacity black over a white/black split measures as
 * `rgb(128, 128, 128)` — which clears 3:1 against *both* halves — while the
 * letters over the black half are `rgb(0, 0, 0)` and invisible. Pairing each
 * pixel's ink with the surface under that pixel is the only reading that
 * survives a split.
 *
 * A surface that holds only a sliver of edge pixels still gets scored: its
 * ink is the best-covered pixel available there, and a sliver of antialiased
 * edge blends toward its own surface, which reads as *lower* contrast. That
 * can only fail a line, never pass one — the opposite of the false pass this
 * replaces.
 */
export function inkBySurface(
    rendered: Shot,
    painted: Shot,
    glyphs: Uint8Array,
    coverage: Float32Array,
    rect: Rect,
): { surface: Rgb; ink: Rgb }[] {
    const { width, height } = rendered;
    const y0 = Math.max(0, Math.floor(rect.y));
    const y1 = Math.min(Math.floor(rect.y + rect.height), height - 1);
    const x0 = Math.max(0, Math.floor(rect.x));
    const x1 = Math.min(Math.floor(rect.x + rect.width), width - 1);
    if (x1 < x0 || y1 < y0) return [];

    // A pixel the glyph barely covers is mostly surface, so its colour is a
    // blend and says nothing reliable about the ink. Scored on its own, such a
    // sliver reports the surface as the ink — contrast 1.00 for text that is
    // perfectly legible. The coverage floor is therefore relative to the best
    // pixel on this line: opaque text peaks at 1.0 and translucent text at its
    // own alpha, so an absolute floor would drop every pixel of faded text and
    // report the line as having nothing to measure.
    let peakCoverage = 0;
    for (let y = y0; y <= y1; y += 1) {
        for (let x = x0; x <= x1; x += 1) {
            const i = y * width + x;
            if (glyphs[i] === 1 && coverage[i] > peakCoverage) peakCoverage = coverage[i];
        }
    }
    const minCoverage = peakCoverage * 0.5;
    const groups = new Map<
        string,
        { surface: Rgb; samples: { distance: number; rgb: Rgb }[] }
    >();
    // Every surface a glyph touches, including those whose only glyph pixels are
    // below the coverage floor. Dropping them would let the audit score only the
    // backgrounds that happened to be measured well: over video each surface
    // colour is nearly unique per pixel, so a bright patch that a letter only
    // grazes would never be scored at all.
    const touched = new Map<string, Rgb>();
    for (let y = y0; y <= y1; y += 1) {
        for (let x = x0; x <= x1; x += 1) {
            const i = y * width + x;
            if (glyphs[i] !== 1) continue;
            const bo = i * painted.channels;
            const surface: Rgb = [
                painted.pixels[bo],
                painted.pixels[bo + 1],
                painted.pixels[bo + 2],
            ];
            const key = surface.join(',');
            if (!touched.has(key)) touched.set(key, surface);
            if (coverage[i] < minCoverage) continue;
            const ro = i * rendered.channels;
            const rgb: Rgb = [rendered.pixels[ro], rendered.pixels[ro + 1], rendered.pixels[ro + 2]];
            const distance =
                Math.abs(rgb[0] - surface[0]) +
                Math.abs(rgb[1] - surface[1]) +
                Math.abs(rgb[2] - surface[2]);
            const group = groups.get(key);
            if (group) group.samples.push({ distance, rgb });
            else groups.set(key, { surface, samples: [{ distance, rgb }] });
        }
    }

    const inkOf = (samples: { distance: number; rgb: Rgb }[]): Rgb => {
        samples.sort((a, b) => b.distance - a.distance);
        const take = Math.max(1, Math.floor(samples.length / 10));
        const best = samples.slice(0, take);
        const sum = best.reduce(
            (acc, s) => [acc[0] + s.rgb[0], acc[1] + s.rgb[1], acc[2] + s.rgb[2]],
            [0, 0, 0],
        );
        return [
            Math.round(sum[0] / best.length),
            Math.round(sum[1] / best.length),
            Math.round(sum[2] / best.length),
        ];
    };

    // The ink this line is drawn in, read from the surface that measured it best.
    // A surface with no above-floor pixels of its own is scored against this: the
    // letters are one colour, and it is the backdrop that changed under them.
    let lineInk: Rgb | null = null;
    let most = -1;
    for (const { samples } of groups.values()) {
        if (samples.length > most) {
            most = samples.length;
            lineInk = inkOf([...samples]);
        }
    }

    const result: { surface: Rgb; ink: Rgb }[] = [];
    for (const [key, surface] of touched) {
        const group = groups.get(key);
        const ink = group ? inkOf(group.samples) : lineInk;
        if (ink) result.push({ surface, ink });
    }
    return result;
}

/**
 * The surface behind the glyphs, plus a mask of which pixels a glyph covers.
 *
 * Three screenshots: the text made transparent, which is the true surface, and
 * the text painted in each of two probe colours, which mark the glyph footprint.
 * A pixel is glyph paint when it differs between the two probe fills.
 *
 * Both details are load-bearing, and getting either wrong is silent.
 *
 * The mask must not be inferred from colour. A filter that keeps pixels
 * resembling the text deletes the case the audit exists for — text painted the
 * same colour as its backdrop — and one that keeps pixels resembling a probe
 * colour counts any backdrop that happens to look like the probe.
 *
 * Diffing one probe against the transparent shot is not enough either: a glyph
 * painted the same colour as a nearby backdrop changes nothing, so the mask is
 * blind exactly there. Diffing the *two probes against each other* removes the
 * dependence on the backdrop entirely — a glyph pixel is probe A in one shot and
 * probe B in the other, so it always differs, whatever is underneath.
 *
 * A real-colour render cannot be one of the two shots for the same reason: the
 * text's own colour may match the backdrop. Both shots are forced, so neither
 * is blind. The forced fill also catches translucent text, which a
 * same-colour-as-rendered diff would miss.
 */
async function glyphLayers(
    page: Page,
    selector: string,
): Promise<{
    rendered: Shot;
    painted: Shot;
    glyphs: Uint8Array;
    coverage: Float32Array;
    runs: TextRun[];
}> {
    const applyFill = async (fill: string | null, freezeOnly = false) => {
        const ok = await page.evaluate(
            ([sel, value, frozen]) => {
                const el = document.querySelector(sel as string);
                if (!el) return false;
                const id = 'contrast-probe-style';
                let style = document.getElementById(id) as HTMLStyleElement | null;
                if (!style) {
                    style = document.createElement('style');
                    style.id = id;
                    document.head.appendChild(style);
                }
                if (value === null && !frozen) {
                    el.removeAttribute('data-contrast-probe');
                    style.textContent = '';
                    return true;
                }
                el.setAttribute('data-contrast-probe', '');
                // `-webkit-text-fill-color` leaves the background, borders and
                // shadows alone, which `color` would not: a `background:
                // currentColor` would vanish and change the very surface being
                // measured. `opacity` is deliberately left alone, so translucent
                // text still shifts when the fill changes.
                // Animations and a blinking caret change the pixels between the
                // three screenshots, so the "surface" and the glyph mask would
                // describe different frames. Freeze them for the probe.
                const fillRule = value === null ? '' : `-webkit-text-fill-color: ${value} !important;`;
                style.textContent =
                    `[data-contrast-probe], [data-contrast-probe] * { ${fillRule}` +
                    ' animation: none !important; transition: none !important; caret-color: transparent !important; }';
                return true;
            },
            [selector, fill, freezeOnly] as [string, string | null, boolean],
        );
        return ok;
    };

    try {
        // The as-rendered frame is taken first and under the same freeze as the
        // probe frames. Capturing geometry before freezing let an entrance
        // animation move the text between the rects and the shots, so the mask
        // was read where the text no longer was.
        await applyFill(null, true);
        const rendered = decodePng(await page.screenshot());
        // Geometry is read here, inside the frozen window, so the rects describe
        // the frame the pixels came from. Reading it after this function returns
        // would restore the animation first, and an entrance transition would
        // then move the text out from under the mask.
        const runs = await textRuns(page, selector);
        if (!(await applyFill('transparent'))) {
            return {
                rendered,
                painted: rendered,
                glyphs: new Uint8Array(0),
                coverage: new Float32Array(0),
                runs,
            };
        }
        const painted = decodePng(await page.screenshot());

        const probes: Shot[] = [];
        for (const fill of PROBE_FILLS) {
            await applyFill(`rgb(${fill.join(', ')})`);
            probes.push(decodePng(await page.screenshot()));
        }


        const [a, b] = probes;
        const glyphs = new Uint8Array(a.width * a.height);
        // How much of the pixel the glyph actually covers. The probe fills are
        // deliberately opposite (`PROBE_FILLS`), so the delta between them
        // scales with coverage: a fully covered pixel is all probe colour and
        // moves the whole distance, an antialiased edge pixel moves only a
        // fraction. Reading ink from edge pixels is what a per-surface score
        // must not do — an edge is a blend of ink and surface, so a faint one
        // reads as the surface itself (contrast 1.00) and a bold one reads as
        // ink that is nowhere on screen.
        const coverage = new Float32Array(a.width * a.height);
        const maxDelta =
            Math.abs(PROBE_FILLS[0][0] - PROBE_FILLS[1][0]) +
            Math.abs(PROBE_FILLS[0][1] - PROBE_FILLS[1][1]) +
            Math.abs(PROBE_FILLS[0][2] - PROBE_FILLS[1][2]);
        for (let i = 0; i < glyphs.length; i += 1) {
            const o = i * a.channels;
            const delta =
                Math.abs(a.pixels[o] - b.pixels[o]) +
                Math.abs(a.pixels[o + 1] - b.pixels[o + 1]) +
                Math.abs(a.pixels[o + 2] - b.pixels[o + 2]);
            if (delta > GLYPH_DELTA) glyphs[i] = 1;
            coverage[i] = Math.min(1, delta / maxDelta);
        }
        return { rendered, painted, glyphs, coverage, runs };
    } finally {
        await applyFill(null);
    }
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
    const { rendered, painted, glyphs, coverage, runs } = await glyphLayers(page, selector);
    expect(runs.length, `no text inside ${label}`).toBeGreaterThan(0);

    let worst: { ratio: number; color: string; bg: Rgb } | null = null;
    let measured = 0;
    for (const run of runs) {
        if (!parseColor(run.color)) continue;
        for (const rect of run.rects) {
            // Each surface under the letters is scored with the ink measured on
            // *that* surface. One ink per run is a false pass on a split: see
            // `inkBySurface`.
            const pairs = inkBySurface(rendered, painted, glyphs, coverage, rect);
            if (pairs.length === 0) continue; // No visible glyphs here.
            measured += 1;
            // Every surface under the letters is scored, with no minimum share.
            // A share threshold is a false-pass by construction: text crossing a
            // narrow unreadable band is exactly the case that matters, and the
            // band is precisely what a threshold discards.
            for (const { surface, ink } of pairs) {
                const ratio = contrastRatio(ink, surface);
                expect(Number.isFinite(ratio), `${label}: non-finite contrast`).toBe(true);
                if (worst === null || ratio < worst.ratio) {
                    worst = { ratio, color: `rgb(${ink.join(', ')})`, bg: surface };
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

