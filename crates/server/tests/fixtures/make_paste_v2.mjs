// Regenerate paste_v2.json: node make_paste_v2.mjs > paste_v2.json
// Encrypts a paste exactly like the PrivateBin v2 JS client (no password).
import { webcrypto as crypto } from 'node:crypto';
import zlib from 'node:zlib';
const ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
function base58(bytes) {
  let n = BigInt('0x' + Buffer.from(bytes).toString('hex')); let s = '';
  while (n > 0n) { s = ALPHABET[Number(n % 58n)] + s; n /= 58n; }
  for (const b of bytes) { if (b === 0) s = '1' + s; else break; }
  return s;
}
const key = crypto.getRandomValues(new Uint8Array(32));
const iv = crypto.getRandomValues(new Uint8Array(16));
const salt = crypto.getRandomValues(new Uint8Array(8));
const b64 = (u) => Buffer.from(u).toString('base64');
const adata = [[b64(iv), b64(salt), 100000, 256, 128, 'aes', 'gcm', 'zlib'], 'plaintext', 0, 0];
const base = await crypto.subtle.importKey('raw', key, 'PBKDF2', false, ['deriveKey']);
const aesKey = await crypto.subtle.deriveKey({ name: 'PBKDF2', salt, iterations: 100000, hash: 'SHA-256' }, base, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
const plaintext = JSON.stringify({ paste: 'Hello from a PrivateBin v2 fixture!' });
const compressed = zlib.deflateRawSync(Buffer.from(plaintext, 'utf8'));
const ct = await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: Buffer.from(JSON.stringify(adata)), tagLength: 128 }, aesKey, compressed);
const body = { v: 2, adata, ct: b64(new Uint8Array(ct)), meta: { expire: '1week' } };
console.log(JSON.stringify({ body, key: base58(key), plaintext }, null, 2));
