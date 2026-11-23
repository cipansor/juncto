import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { Page } from '@playwright/test';
import { expect, test } from '@playwright/test';
import { capture, loadCaptured, manifestPath } from './gallery-manifest';

test.describe('gallery manifest', () => {
    let saved: string | null = null;

    test.beforeEach(() => {
        const p = manifestPath();
        saved = fs.existsSync(p) ? fs.readFileSync(p, 'utf8') : null;
        fs.writeFileSync(p, '[]');
    });

    test.afterEach(() => {
        const p = manifestPath();
        if (saved === null) fs.rmSync(p, { force: true });
        else fs.writeFileSync(p, saved);
    });

    test('a capture that writes no file does not vouch for itself', async () => {
        // The manifest is what a retry loads in a fresh worker. Recording the
        // name before the image is on disk makes the retry report the view
        // captured while its screenshot is missing, so a broken run goes green.
        const captured: string[] = [];
        const written: string[] = [];
        const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'gallery-fail-'));
        const broken = {
            screenshot: async () => {
                throw new Error('disk full');
            },
        } as unknown as Page;

        await expect(capture(broken, '99-broken', written, captured, dir)).rejects.toThrow(
            'disk full',
        );
        expect(captured, 'a failed write must not be recorded').toEqual([]);
        expect(loadCaptured(), 'the manifest must not name a view with no image').toEqual([]);
    });

    test('a capture that writes its file is recorded once', async () => {
        const captured: string[] = [];
        const written: string[] = [];
        const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'gallery-ok-'));
        const ok = {
            screenshot: async ({ path: p }: { path: string }) => {
                fs.writeFileSync(p, 'png');
            },
        } as unknown as Page;

        await capture(ok, '01-home', written, captured, dir);
        await capture(ok, '01-home', written, captured, dir);

        expect(loadCaptured(), 'the manifest records the view').toEqual(['01-home']);
        expect(written, 'the append-only list keeps every attempt').toEqual([
            '01-home',
            '01-home',
        ]);
    });

});
