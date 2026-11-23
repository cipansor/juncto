import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { Page } from '@playwright/test';

/**
 * The names captured so far, kept on disk as well as in memory.
 *
 * A retry runs in a fresh worker with a fresh module instance, so an in-memory
 * list is empty on the second attempt and `afterAll` then reports the first
 * attempt's views as missing — failing a run whose files are all present. The
 * manifest is scoped to the run id so a stale manifest from an earlier run
 * cannot vouch for a view this run never captured.
 */
export function manifestPath(): string {
    return path.join(
        os.tmpdir(),
        `juncto-gallery-${process.env.E2E_RUN_ID ?? 'local'}.json`,
    );
}

export function loadCaptured(): string[] {
    try {
        const raw = JSON.parse(fs.readFileSync(manifestPath(), 'utf8')) as unknown;
        return Array.isArray(raw) ? raw.filter((n): n is string => typeof n === 'string') : [];
    } catch {
        return [];
    }
}

export function recordCaptured(names: string[]) {
    fs.writeFileSync(manifestPath(), JSON.stringify(names));
}

/**
 * Capture one view and remember its name. Every capture goes through here so
 * the uniqueness check sees the declared name even when the write replaces an
 * earlier file, and so a silently skipped capture cannot pass unnoticed.
 *
 * `written` is append-only: the duplicate-number check reads it, and deduping
 * would hide a spec that captured the same number twice. `captured` is the
 * persisted manifest, and it is updated only once the file is on disk — a name
 * recorded before the write can fail would let a broken capture vouch for
 * itself on a retry, which loads the manifest and reports the view captured
 * although no image for it exists.
 */
export async function capture(
    target: Page,
    name: string,
    written: string[],
    captured: string[],
    dir: string,
) {
    written.push(name);
    await target.screenshot({ path: `${dir}/${name}.png` });
    if (!fs.existsSync(`${dir}/${name}.png`)) {
        throw new Error(`capture ${name} did not write a file`);
    }
    if (!captured.includes(name)) {
        captured.push(name);
        recordCaptured(captured);
    }
}
