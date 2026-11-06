import { expect, test as base, type Page } from '@playwright/test';

/** Fails the test on any console error, uncaught exception or CSP violation. */
export const test = base.extend<{ problems: string[] }>({
  problems: async ({ context }, use) => {
    const problems: string[] = [];
    await context.addInitScript(() =>
      document.addEventListener('securitypolicyviolation', (e) => console.error(`CSP violation: ${e.violatedDirective} ${e.blockedURI}`)),
    );
    context.on('console', (m) => {
      if (m.type() === 'error') problems.push(m.text());
    });
    context.on('weberror', (e) => problems.push(e.error().message));
    await use(problems);
    expect(problems, 'console errors / CSP violations').toEqual([]);
  },
  page: async ({ page, problems }, use) => {
    void problems;
    await use(page);
  },
});
export { expect };

export interface CreateOptions {
  text: string;
  format?: 'Plain text' | 'Code' | 'Markdown';
  burn?: boolean;
  discussion?: boolean;
  password?: string;
  file?: { name: string; mimeType: string; buffer: Buffer };
}

/** Fills the editor, submits and returns the share link shown on the "link ready" screen. */
export async function createPaste(page: Page, opts: CreateOptions): Promise<string> {
  await page.goto('/');
  await page.fill('#paste-text', opts.text);
  if (opts.format) await page.getByRole('radio', { name: opts.format }).click();
  if (opts.burn) await page.getByRole('switch', { name: 'Burn after reading' }).click();
  if (opts.discussion) await page.getByRole('switch', { name: 'Open discussion' }).click();
  if (opts.password) await page.fill('#password', opts.password);
  if (opts.file) await page.setInputFiles('input[type=file]', opts.file);
  await page.getByRole('button', { name: /Encrypt & create link/ }).click();
  await expect(page.getByRole('heading', { name: 'Your link is ready' })).toBeVisible({ timeout: 15_000 });
  return (await page.locator('.share-url').innerText()).replace(/\s/g, '');
}

/** A 1x1 transparent PNG. */
export const tinyPng = {
  name: 'pixel.png',
  mimeType: 'image/png',
  buffer: Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==', 'base64'),
};
