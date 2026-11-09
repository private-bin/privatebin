// Crypto compatibility tests (run with `npm test`, Node >= 22).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { base58Decode, base58Encode } from '../src/base58';
import { DecryptionError, decrypt, encryptComment, encryptPaste, randomBytes } from '../src/crypto';

const fixture = JSON.parse(readFileSync(new URL('../../crates/server/tests/fixtures/paste_v2.json', import.meta.url), 'utf8'));

function keyFrom(text: string): Uint8Array {
  const bytes = base58Decode(text);
  const key = new Uint8Array(32);
  key.set(bytes, 32 - bytes.length);
  return key;
}

test('base58 round-trips, including leading zero bytes', () => {
  for (let i = 0; i < 200; i++) {
    const bytes = randomBytes(32);
    if (i % 4 === 0) bytes[0] = 0;
    if (i % 8 === 0) bytes[1] = 0;
    assert.deepEqual(base58Decode(base58Encode(bytes)), bytes);
  }
  assert.equal(base58Encode(new Uint8Array([0, 0, 1])), '112');
  assert.throws(() => base58Decode('0OIl'));
});

test('decrypts the v2 fixture produced like the official client', async () => {
  const message = await decrypt(keyFrom(fixture.key), '', fixture.body.ct, fixture.body.adata);
  assert.deepEqual(message, JSON.parse(fixture.plaintext));
});

test('wrong key fails with DecryptionError', async () => {
  await assert.rejects(decrypt(randomBytes(32), '', fixture.body.ct, fixture.body.adata), DecryptionError);
});

test('tampered adata fails authentication', async () => {
  const adata = structuredClone(fixture.body.adata);
  adata[3] = 1; // flip burn-after-reading
  await assert.rejects(decrypt(keyFrom(fixture.key), '', fixture.body.ct, adata), DecryptionError);
});

test('paste round-trip with password, unicode and attachment arrays', async () => {
  const key = randomBytes(32);
  const message = { paste: 'xin chào 👋\n'.repeat(50), attachment: ['data:text/plain;base64,aGk='], attachment_name: ['hi.txt'] };
  const { ct, adata } = await encryptPaste(key, 'mật khẩu', message, 'markdown', true, false);
  assert.equal(adata[1], 'markdown');
  assert.equal(adata[2], 1);
  assert.equal(adata[0][7], 'zlib');
  // simulate the server JSON round-trip: AAD must survive serialization unchanged
  const wire = JSON.parse(JSON.stringify({ ct, adata }));
  assert.deepEqual(await decrypt(key, 'mật khẩu', wire.ct, wire.adata), message);
  await assert.rejects(decrypt(key, 'wrong', wire.ct, wire.adata), DecryptionError);
  await assert.rejects(decrypt(key, '', wire.ct, wire.adata), DecryptionError);
});

test('comment adata binds paste and parent ids', async () => {
  const key = randomBytes(32);
  const { ct, adata } = await encryptComment(key, '', { comment: 'hi', nickname: 'lan' }, 'aaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbb');
  assert.equal(adata.length, 10);
  assert.deepEqual(adata.slice(8), ['aaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbb']);
  assert.deepEqual(await decrypt(key, '', ct, adata), { comment: 'hi', nickname: 'lan' });
  const moved = [...adata.slice(0, 8), 'aaaaaaaaaaaaaaaa', 'cccccccccccccccc'];
  await assert.rejects(decrypt(key, '', ct, moved), DecryptionError);
});

test('rejects out-of-range cipher parameters before deriving a key', async () => {
  const key = randomBytes(32);
  const { ct, adata } = await encryptPaste(key, '', { paste: 'x' }, 'plaintext', false, false);
  const started = Date.now();
  for (const [index, value] of [
    [2, 4_000_000_000],
    [2, 1],
    [3, 512],
    [4, 32],
    [6, 'cbc'],
    [0, 'AAAA'],
  ] as const) {
    const bad = structuredClone(adata) as unknown[][];
    bad[0][index] = value;
    await assert.rejects(decrypt(key, '', ct, bad), DecryptionError);
  }
  assert.ok(Date.now() - started < 2000, 'validation must happen before PBKDF2');
});

test('caps the decompressed size', async () => {
  const key = randomBytes(32);
  const { ct, adata } = await encryptPaste(key, '', { paste: 'a'.repeat(2_000_000) }, 'plaintext', false, false);
  assert.ok(ct.length < 50_000, 'payload compresses well');
  await assert.rejects(decrypt(key, '', ct, adata, 1_000_000), DecryptionError);
  const message = await decrypt<{ paste: string }>(key, '', ct, adata);
  assert.equal(message.paste.length, 2_000_000);
});

test('generated passwords are 12 unambiguous characters', async () => {
  const { PASSWORD_ALPHABET, generatePassword } = await import('../src/crypto');
  const seen = new Set<string>();
  for (let i = 0; i < 500; i++) {
    const pw = generatePassword();
    assert.equal(pw.length, 12);
    for (const c of pw) assert.ok(PASSWORD_ALPHABET.includes(c), c);
    seen.add(pw);
  }
  assert.equal(seen.size, 500);
  assert.ok(!/[0O1lI]/.test(PASSWORD_ALPHABET));
});
