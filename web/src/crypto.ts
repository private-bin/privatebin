// Client-side encryption, byte-compatible with PrivateBin's v2 format (js/privatebin.js, CryptTool).
//
// key material = 32 random bytes (+ UTF-8 password bytes) → PBKDF2-SHA256 → AES-GCM key.
// plaintext    = UTF-8 JSON, raw-deflate compressed when spec[7] === 'zlib'.
// AAD          = JSON.stringify(adata) exactly as sent to / received from the server.

export type CipherSpec = [string, string, number, number, number, string, string, string];
export type PasteAdata = [CipherSpec, string, number, number];
/** 8 elements (older clients) or 10 elements: spec + [pasteId, parentId]. */
export type CommentAdata = (string | number)[];

export class DecryptionError extends Error {
  constructor() {
    super('Could not decrypt the data');
    this.name = 'DecryptionError';
  }
}

const ITERATIONS = 100_000;
/** Accepted PBKDF2 iteration range for received data (the server enforces the same). */
const MIN_ITERATIONS = 10_000;
const MAX_ITERATIONS = 1_000_000;
/** Default cap on decompressed plaintext, against compression bombs. */
export const MAX_PLAINTEXT_BYTES = 100 * 1024 * 1024;
const KEY_BITS = 256;
const TAG_BITS = 128;
const encoder = new TextEncoder();

export function randomBytes(length: number): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(length));
}

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export function base64ToBytes(text: string): Uint8Array {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** Older PrivateBin versions truncated each UTF-16 code unit of the password to its low byte. */
function legacyPasswordBytes(password: string): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(password, (c) => c.charCodeAt(0) & 0xff);
}

async function pipe(data: Uint8Array, stream: CompressionStream): Promise<Uint8Array<ArrayBuffer>> {
  const output = new Blob([data as BlobPart]).stream().pipeThrough(stream);
  return new Uint8Array(await new Response(output).arrayBuffer());
}

/** Decompresses, aborting as soon as the output exceeds `maxBytes`. */
async function inflateLimited(data: Uint8Array, maxBytes: number): Promise<Uint8Array<ArrayBuffer>> {
  const reader = new Blob([data as BlobPart]).stream().pipeThrough(new DecompressionStream('deflate-raw')).getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.length;
    if (total > maxBytes) {
      await reader.cancel();
      throw new DecryptionError();
    }
    chunks.push(value);
  }
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}

async function deriveKey(key: Uint8Array, password: Uint8Array, spec: CipherSpec): Promise<CryptoKey> {
  const material = new Uint8Array(key.length + password.length);
  material.set(key, 0);
  material.set(password, key.length);
  const base = await crypto.subtle.importKey('raw', material as BufferSource, { name: 'PBKDF2' }, false, ['deriveKey']);
  return crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt: base64ToBytes(spec[1]) as BufferSource, iterations: spec[2], hash: { name: 'SHA-256' } },
    base,
    { name: 'AES-' + spec[6].toUpperCase(), length: spec[3] },
    false,
    ['encrypt', 'decrypt'],
  );
}

function aesParams(spec: CipherSpec, adata: unknown): AesGcmParams {
  return {
    name: 'AES-GCM',
    iv: base64ToBytes(spec[0]) as BufferSource,
    additionalData: encoder.encode(JSON.stringify(adata)) as BufferSource,
    tagLength: spec[4],
  };
}

function newSpec(): CipherSpec {
  return [bytesToBase64(randomBytes(16)), bytesToBase64(randomBytes(8)), ITERATIONS, KEY_BITS, TAG_BITS, 'aes', 'gcm', 'zlib'];
}

async function encryptWith(key: Uint8Array, password: string, message: string, spec: CipherSpec, adata: unknown): Promise<string> {
  let data = encoder.encode(message);
  if (spec[7] === 'zlib') data = await pipe(data, new CompressionStream('deflate-raw'));
  const cipherKey = await deriveKey(key, encoder.encode(password), spec);
  const ct = await crypto.subtle.encrypt(aesParams(spec, adata), cipherKey, data as BufferSource);
  return bytesToBase64(new Uint8Array(ct));
}

export async function encryptPaste(
  key: Uint8Array,
  password: string,
  message: object,
  formatter: string,
  openDiscussion: boolean,
  burnAfterReading: boolean,
): Promise<{ ct: string; adata: PasteAdata }> {
  const spec = newSpec();
  const adata: PasteAdata = [spec, formatter, openDiscussion ? 1 : 0, burnAfterReading ? 1 : 0];
  return { ct: await encryptWith(key, password, JSON.stringify(message), spec, adata), adata };
}

export async function encryptComment(
  key: Uint8Array,
  password: string,
  message: object,
  pasteId: string,
  parentId: string,
): Promise<{ ct: string; adata: CommentAdata }> {
  const spec = newSpec();
  const adata: CommentAdata = [...spec, pasteId, parentId];
  return { ct: await encryptWith(key, password, JSON.stringify(message), spec, adata), adata };
}

function base64Length(value: unknown): number {
  if (typeof value !== 'string') return -1;
  try {
    return base64ToBytes(value).length;
  } catch {
    return -1;
  }
}

/**
 * Extracts and validates the cipher spec from received adata. It comes from the
 * server and is checked before the GCM tag, so every parameter is bounded: a huge
 * iteration count would otherwise stall the browser before anything is authenticated.
 */
function specOf(adata: unknown): CipherSpec {
  if (!Array.isArray(adata)) throw new DecryptionError();
  const spec: unknown[] = Array.isArray(adata[0]) ? adata[0] : adata.slice(0, 8);
  const [iv, salt, iterations, keySize, tagSize, algorithm, mode, compression] = spec;
  const valid =
    spec.length === 8 &&
    base64Length(iv) >= 12 &&
    base64Length(iv) <= 16 &&
    base64Length(salt) >= 8 &&
    base64Length(salt) <= 16 &&
    Number.isInteger(iterations) &&
    (iterations as number) >= MIN_ITERATIONS &&
    (iterations as number) <= MAX_ITERATIONS &&
    (keySize === 128 || keySize === 192 || keySize === 256) &&
    (tagSize === 96 || tagSize === 128) &&
    algorithm === 'aes' &&
    mode === 'gcm' &&
    (compression === 'zlib' || compression === 'none');
  if (!valid) throw new DecryptionError();
  return spec as CipherSpec;
}

/** Decrypts a v2 paste or comment and returns the decoded JSON message. */
export async function decrypt<T = Record<string, unknown>>(
  key: Uint8Array,
  password: string,
  ct: string,
  adata: unknown,
  maxPlaintextBytes = MAX_PLAINTEXT_BYTES,
): Promise<T> {
  const spec = specOf(adata);
  const attempts: Uint8Array[] = [encoder.encode(password)];
  if (/[\u0080-￿]/.test(password)) attempts.push(legacyPasswordBytes(password));

  let plain: Uint8Array<ArrayBuffer> | null = null;
  for (const attempt of attempts) {
    try {
      const cipherKey = await deriveKey(key, attempt, spec);
      plain = new Uint8Array(await crypto.subtle.decrypt(aesParams(spec, adata), cipherKey, base64ToBytes(ct) as BufferSource));
      break;
    } catch {
      // try the next password encoding
    }
  }
  if (!plain) throw new DecryptionError();

  try {
    if (spec[7] === 'zlib') plain = await inflateLimited(plain, maxPlaintextBytes);
    else if (plain.length > maxPlaintextBytes) throw new DecryptionError();
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(plain)) as T;
  } catch {
    throw new DecryptionError();
  }
}
