import { createPaste, expect, test, tinyPng } from './fixtures';

test('code paste with QR code, highlighting and nested comments', async ({ page, browser }) => {
  const code = 'fn main() {\n    let greeting = "xin chào";\n    println!("{greeting}, PrivateBin!");\n}\n';
  const url = await createPaste(page, { text: code, format: 'Code', discussion: true });
  expect(url).toMatch(/\?[0-9a-f]{16}#[1-9A-HJ-NP-Za-km-z]{40,}$/);

  await page.getByRole('button', { name: 'Show QR code' }).click();
  await expect(page.locator('.qr svg path')).toHaveCount(1);

  await page.getByRole('button', { name: 'Open paste' }).click();
  await expect(page.locator('.code__line')).toHaveCount(4);
  await expect(page.locator('.code [class^=hljs]').first()).toBeVisible();

  await page.fill('#nick', 'lan');
  await page.fill('#comment', 'Bình luận đầu tiên');
  await page.getByRole('button', { name: 'Post comment' }).click();
  await expect(page.locator('.comment__text', { hasText: 'Bình luận đầu tiên' })).toBeVisible();
  await page.getByRole('button', { name: 'Reply' }).first().click();
  await page.fill('#comment', 'A reply');
  await page.getByRole('button', { name: 'Post comment' }).click();
  await expect(page.locator('.comment-children .comment__text', { hasText: 'A reply' })).toBeVisible();

  // A different browser profile decrypts the paste and the comments from the link alone.
  const other = await (await browser.newContext({ colorScheme: 'dark' })).newPage();
  await other.goto(url);
  await expect(other.locator('.comment__text', { hasText: 'A reply' })).toBeVisible();
  await expect(other.locator('.code__line')).toHaveCount(4);
});

test('theme toggle persists dark mode', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /Switch to dark mode/ }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('markdown is sanitized and image attachments are previewed', async ({ page }) => {
  const url = await createPaste(page, {
    // No event-handler attributes in the payload: Chrome reports a CSP violation as soon as DOMPurify's
    // inert parser sees one, even though it never runs and is stripped from the output.
    text: '# Title\n\n- **bold** item\n\n<script>alert(1)</script>\n\n[x](javascript:alert(1)) [ok](https://example.com)',
    format: 'Markdown',
    file: tinyPng,
  });
  await page.goto('about:blank');
  await page.goto(url);
  await expect(page.locator('.markdown h1')).toHaveText('Title');
  await expect(page.locator('.markdown strong')).toHaveCount(1);
  await expect(page.locator('.markdown script')).toHaveCount(0);
  await expect(page.locator('.markdown a[href^="javascript:"]')).toHaveCount(0);
  await expect(page.locator('.markdown a[href="https://example.com"]')).toHaveAttribute('rel', 'noopener noreferrer nofollow');
  await expect(page.locator('.attachment__preview img')).toHaveCount(1);
});

test('password gate rejects a wrong password and accepts the right one', async ({ page, browser }) => {
  const url = await createPaste(page, { text: 'top secret', password: 'mật khẩu' });
  const reader = await (await browser.newContext()).newPage();
  await reader.goto(url);
  await reader.fill('#pw', 'wrong');
  await reader.getByRole('button', { name: 'Decrypt' }).click();
  await expect(reader.locator('#pw-error')).toBeVisible();
  await reader.fill('#pw', 'mật khẩu');
  await reader.getByRole('button', { name: 'Decrypt' }).click();
  await expect(reader.locator('.content__plain')).toHaveText('top secret');
});

test('burn after reading can be opened exactly once', async ({ page, browser }) => {
  const url = await createPaste(page, { text: 'read me once', burn: true });
  expect(url).toContain('#-');
  await expect(page.getByRole('button', { name: 'Open paste' })).toHaveCount(0);

  const first = await (await browser.newContext()).newPage();
  await first.goto(url);
  await expect(first.getByRole('heading', { name: 'This paste can be read only once' })).toBeVisible();
  await first.getByRole('button', { name: 'Open and delete' }).click();
  await expect(first.locator('.content__plain')).toHaveText('read me once');
  await expect(first.locator('.callout--warn')).toHaveCount(1);

  const second = await (await browser.newContext()).newPage();
  await second.goto(url);
  await second.getByRole('button', { name: 'Open and delete' }).click();
  await expect(second.getByRole('heading', { name: 'This paste is gone' })).toBeVisible();
});

test('delete link removes the paste', async ({ page, browser }) => {
  const url = await createPaste(page, { text: 'delete me' });
  await page.getByRole('button', { name: 'Delete now' }).click();
  await page.getByRole('button', { name: 'Click again to delete' }).click();
  await expect(page.getByRole('heading', { name: 'The paste was deleted' })).toBeVisible();

  const reader = await (await browser.newContext()).newPage();
  await reader.goto(url);
  await expect(reader.getByRole('heading', { name: 'This paste is gone' })).toBeVisible();
});

test('link without key shows an explanation', async ({ page }) => {
  const url = await createPaste(page, { text: 'no key' });
  await page.goto(url.split('#')[0]);
  await expect(page.getByRole('heading', { name: 'This link has no key' })).toBeVisible();
});

test('empty paste is rejected client-side', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /Encrypt & create link/ }).click();
  await expect(page.locator('.callout--danger')).toContainText('Write something');
});

test('entry assets carry subresource integrity', async ({ page }) => {
  await page.goto('/');
  const integrity = await page.locator('script[src^="/assets/"], link[rel=stylesheet]').evaluateAll((els) => els.map((e) => e.getAttribute('integrity')));
  expect(integrity).toHaveLength(3);
  for (const value of integrity) expect(value).toMatch(/^sha384-/);
});

test.describe('mobile', () => {
  test.use({ viewport: { width: 390, height: 844 } });
  test('create page has no horizontal overflow', async ({ page }) => {
    await page.goto('/');
    expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
  });
});

test('HTML attachments download as opaque files instead of running in the app origin', async ({ page }) => {
  const url = await createPaste(page, {
    text: 'see attachment',
    file: { name: 'report.html', mimeType: 'text/html', buffer: Buffer.from('<script>document.title="PWNED"</script>') },
  });
  await page.goto('about:blank');
  await page.goto(url);
  const link = page.getByRole('link', { name: 'Download' });
  await expect(link).toHaveAttribute('download', 'report.html');
  // The attack: open the attachment link in a new tab. It must download, not render.
  const href = (await link.getAttribute('href'))!;
  const tab = await page.context().newPage();
  const download = tab.waitForEvent('download');
  await tab.goto(href).catch(() => {}); // navigation is aborted by the download
  await download;
  expect(await tab.title()).not.toBe('PWNED');
});

test('markdown cannot embed SVG links or reuse app classes', async ({ page }) => {
  const url = await createPaste(page, {
    text: '<svg><a href="#-1111"><text y="20">click</text></a></svg>\n\n<div class="toast" id="comment">fake</div>',
    format: 'Markdown',
  });
  await page.goto('about:blank');
  await page.goto(url);
  await expect(page.locator('.markdown')).toContainText('fake');
  await expect(page.locator('.markdown svg')).toHaveCount(0);
  await expect(page.locator('.markdown [class], .markdown [id]')).toHaveCount(0);
});

test('delete links ask for confirmation first', async ({ page, browser }) => {
  await createPaste(page, { text: 'delete via link' });
  const deleteUrl = await page.locator('.share-url ~ *, .mono').filter({ hasText: 'deletetoken=' }).first().innerText();
  const visitor = await (await browser.newContext()).newPage();
  await visitor.goto(deleteUrl.trim());
  await expect(visitor.getByRole('heading', { name: 'Delete this paste?' })).toBeVisible();
  await visitor.getByRole('button', { name: 'Delete paste' }).click();
  await expect(visitor.getByRole('heading', { name: 'The paste was deleted' })).toBeVisible();
});

test('theme toggle follows system theme changes', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Switch to dark mode' })).toBeVisible();
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect(page.getByRole('button', { name: 'Switch to light mode' })).toBeVisible();
});

test('footer links to the source repository', async ({ page }) => {
  await page.goto('/');
  const link = page.getByRole('contentinfo').getByRole('link', { name: 'Source on GitHub' });
  await expect(link).toHaveAttribute('href', 'https://github.com/private-bin/privatebin');
  await expect(link).toHaveAttribute('rel', 'noopener noreferrer');
});
