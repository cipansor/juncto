import { expect, Page } from '@playwright/test';

/** WCAG AA floor for normal-size body text. */
export const AA_TEXT = 4.5;

type Rgba = [number, number, number, number];

function parseColor(value: string): Rgba | null {
    const m = value.match(/rgba?\(([^)]+)\)/);
    if (!m) return null;
    const parts = m[1].split(',').map((p) => parseFloat(p.trim()));
    return [parts[0], parts[1], parts[2], parts[3] ?? 1];
}

function relativeLuminance([r, g, b]: Rgba): number {
    const channel = (c: number) => {
        const s = c / 255;
        return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
    };
    return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function contrastRatio(a: Rgba, b: Rgba): number {
    const l1 = relativeLuminance(a);
    const l2 = relativeLuminance(b);
    const [hi, lo] = l1 > l2 ? [l1, l2] : [l2, l1];
    return (hi + 0.05) / (lo + 0.05);
}

/** Composite an element's background up the tree until it is opaque. */
export async function effectiveBackground(page: Page, selector: string): Promise<string> {
    return page.$eval(selector, (el) => {
        const parse = (v: string): Rgba | null => {
            const m = v.match(/rgba?\(([^)]+)\)/);
            if (!m) return null;
            const p = m[1].split(',').map((x) => parseFloat(x.trim()));
            return [p[0], p[1], p[2], p[3] ?? 1] as Rgba;
        };
        const overlay = (top: Rgba, under: Rgba): Rgba => {
            const a = top[3];
            return [
                top[0] * a + under[0] * (1 - a),
                top[1] * a + under[1] * (1 - a),
                top[2] * a + under[2] * (1 - a),
                1,
            ];
        };

        let acc: Rgba | null = null;
        let node: Element | null = el;
        while (node) {
            const bg = parse(getComputedStyle(node).backgroundColor);
            if (bg && bg[3] > 0) {
                acc = acc === null ? bg : overlay(acc, bg);
                if (acc[3] >= 1) break;
            }
            node = node.parentElement;
        }
        if (!acc) return 'rgb(255, 255, 255)';
        return `rgb(${Math.round(acc[0])}, ${Math.round(acc[1])}, ${Math.round(acc[2])})`;
    });
}

/**
 * Assert an element's text clears the WCAG AA contrast floor against whatever
 * is actually painted behind it. Checking the colour alone does not catch a
 * theme where the text and its surface converge.
 */
export async function expectReadableText(
    page: Page,
    selector: string,
    label: string,
    minRatio = AA_TEXT,
): Promise<number> {
    const locator = page.locator(selector).first();
    const textColor = await locator.evaluate((el) => getComputedStyle(el).color);
    const bgColor = await effectiveBackground(page, selector);
    const fg = parseColor(textColor);
    const bg = parseColor(bgColor);
    expect(fg, `unparseable foreground for ${label}`).not.toBeNull();
    expect(bg, `unparseable background for ${label}`).not.toBeNull();
    const ratio = contrastRatio(fg!, bg!);
    expect(
        ratio,
        `${label}: contrast ${ratio.toFixed(2)} (${textColor} on ${bgColor}) is below ${minRatio}`,
    ).toBeGreaterThanOrEqual(minRatio);
    return ratio;
}
