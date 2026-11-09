// Paste viewer: key handling, decryption, content, attachments and discussion.
import { ApiError, CommentRecord, PasteRecord, getPaste, postComment } from '../api';
import { base58Decode } from '../base58';
import { DecryptionError, base64ToBytes, decrypt, encryptComment } from '../crypto';
import { clear, copyText, downloadBlob, h, prefersReducedMotion, toast } from '../dom';
import { expiresIn, formatBytes, formatterLabel, timeAgo } from '../format';
import { icon } from '../icons';
import { renderContent } from '../render';
import { goHome } from './create';
import { showBurnGate, showError, showLoading, showPasswordGate } from './gates';
import { mountPage } from './layout';

interface PasteMessage {
  paste?: string;
  attachment?: string | string[];
  attachment_name?: string | string[];
}

interface CommentMessage {
  comment?: string;
  nickname?: string;
}

interface Attachment {
  name: string;
  blob: Blob;
}

/** Reads the key from the URL fragment: `#<key>` or `#-<key>` (burn confirmation), ignoring `&…` junk. */
function keyFromHash(): { text: string; confirm: boolean } {
  const hash = location.hash;
  const confirm = hash.startsWith('#-');
  let text = hash.slice(confirm ? 2 : 1);
  const amp = text.indexOf('&');
  if (amp > -1) text = text.slice(0, amp);
  return { text, confirm };
}

/** A 32-byte key is 43–44 base58 characters. Clients that drop leading zero bytes can make it
 *  a little shorter (one byte in 256 keys), but anything under 40 characters was cut off. */
const MIN_KEY_CHARS = 40;

function parseKey(text: string): Uint8Array | null {
  if (text.length < MIN_KEY_CHARS) return null;
  try {
    const bytes = base58Decode(text);
    if (bytes.length === 0 || bytes.length > 32) return null;
    const key = new Uint8Array(32);
    key.set(bytes, 32 - bytes.length);
    return key;
  } catch {
    return null;
  }
}

export async function showPaste(id: string): Promise<void> {
  const { text, confirm } = keyFromHash();
  if (!text) return showError('nokey');
  const key = parseKey(text);
  if (!key) return showError('badkey');

  const load = async () => {
    showLoading('Decrypting…');
    let paste: PasteRecord;
    try {
      paste = await getPaste(id);
    } catch (err) {
      // Only an answer from the API itself means the paste is gone; anything else is a
      // connection or proxy problem, and the paste may well still be there.
      if (err instanceof ApiError && err.kind === 'server') return showError('notfound');
      return showError('unreachable', { detail: err instanceof ApiError && err.kind === 'http' ? `${err.message} The paste was not opened; try again in a moment.` : undefined, retry: () => void load() });
    }
    // The server has now deleted a burn-after-reading paste. Drop the `-` from the
    // fragment so a reload reports it as gone instead of offering to open it again.
    if (confirm) history.replaceState(history.state, '', `${location.pathname}${location.search}#${text}`);
    await unlock(paste, key, '');
  };

  if (confirm) showBurnGate(() => void load());
  else await load();
}

async function unlock(paste: PasteRecord, key: Uint8Array, password: string): Promise<void> {
  try {
    const message = await decrypt<PasteMessage>(key, password, paste.ct, paste.adata);
    await renderPaste(paste, key, password, message);
  } catch (err) {
    if (!(err instanceof DecryptionError)) throw err;
    showPasswordGate({ wrong: password !== '', burned: paste.adata[3] === 1 }, (pw) => void unlock(paste, key, pw));
  }
}

function dataUrlToBlob(dataUrl: string): Blob | null {
  const match = /^data:([^;,]*)((?:;[^;,]*)*),(.*)$/s.exec(dataUrl);
  if (!match) return null;
  const mime = match[1] || 'application/octet-stream';
  try {
    const bytes = match[2].includes(';base64') ? base64ToBytes(match[3]) : new TextEncoder().encode(decodeURIComponent(match[3]));
    return new Blob([bytes as BlobPart], { type: mime });
  } catch {
    return null;
  }
}

function attachmentsOf(message: PasteMessage): Attachment[] {
  const data = message.attachment === undefined ? [] : Array.isArray(message.attachment) ? message.attachment : [message.attachment];
  const names = message.attachment_name === undefined ? [] : Array.isArray(message.attachment_name) ? message.attachment_name : [message.attachment_name];
  const out: Attachment[] = [];
  data.forEach((item, i) => {
    const blob = typeof item === 'string' ? dataUrlToBlob(item) : null;
    const name = typeof names[i] === 'string' ? cleanFileName(names[i]) : '';
    if (blob) out.push({ name: name || `attachment-${i + 1}`, blob });
  });
  return out;
}

/** Drops control and bidirectional-override characters, which can disguise an extension (`exe.txt` shown for `txt.exe`). */
function cleanFileName(name: string): string {
  return name.replace(/[\u0000-\u001f\u007f\u200e\u200f\u202a-\u202e\u2066-\u2069]/g, '').trim();
}

/** Media types shown inline. Anything else is served as an opaque download: a same-origin blob: URL
 *  of type text/html or image/svg+xml would run the paste author's script when opened in a tab. */
const PREVIEWABLE = /^(image\/(png|jpe?g|gif|webp|avif|bmp)|video\/(mp4|webm|ogg)|audio\/(mpeg|ogg|wav|webm|mp4))$/;

function renderAttachment(att: Attachment): HTMLElement {
  const type = att.blob.type;
  const safe = PREVIEWABLE.test(type) ? att.blob : new Blob([att.blob], { type: 'application/octet-stream' });
  const url = URL.createObjectURL(safe);
  const kind = type.startsWith('image/') ? 'image' : 'file';
  const download = h('a', { href: url, download: att.name, class: 'btn btn--sm' }, icon('download', 16, 2), 'Download');
  let preview: HTMLElement | null = null;
  if (PREVIEWABLE.test(type)) {
    if (type.startsWith('image/')) preview = h('img', { src: url, alt: att.name });
    else if (type.startsWith('video/')) preview = h('video', { src: url, controls: true, preload: 'metadata' });
    else preview = h('audio', { src: url, controls: true, preload: 'metadata' });
  }
  return h(
    'section',
    { class: 'card card--clip', 'aria-label': `Attachment ${att.name}` },
    h(
      'div',
      { class: 'attachment__head' },
      h(
        'div',
        { class: 'attachment__info' },
        h('span', { class: 'attachment__icon' }, icon(kind === 'image' ? 'image' : 'file', 18, 1.8)),
        h(
          'div',
          { class: 'stack-xs truncate' },
          h('span', { class: 'truncate small' }, h('strong', null, att.name)),
          h('span', { class: 'small muted' }, `${type || 'File'} · ${formatBytes(att.blob.size)} · encrypted attachment`),
        ),
      ),
      download,
    ),
    preview ? h('div', { class: 'attachment__preview' }, preview) : null,
  );
}

function extensionFor(formatter: string): string {
  return formatter === 'markdown' ? 'md' : 'txt';
}

async function renderPaste(paste: PasteRecord, key: Uint8Array, password: string, message: PasteMessage): Promise<void> {
  const text = typeof message.paste === 'string' ? message.paste : '';
  const [, formatter, discussion, burn] = paste.adata;
  const attachments = attachmentsOf(message);

  const action = (label: string, handler: () => void) => {
    const b = h('button', { type: 'button', class: 'btn btn--sm' }, label);
    b.addEventListener('click', handler);
    return b;
  };
  const textBlob = () => new Blob([text], { type: 'text/plain;charset=utf-8' });
  const actions = text
    ? [
        action('Copy', async () => toast((await copyText(text)) ? 'Copied to clipboard' : 'Copy failed')),
        action('Raw', () => {
          const url = URL.createObjectURL(textBlob());
          window.open(url, '_blank', 'noopener');
          setTimeout(() => URL.revokeObjectURL(url), 60_000);
        }),
        action('Download', () => downloadBlob(textBlob(), `paste-${paste.id}.${extensionFor(formatter)}`)),
        action('Clone', () => {
          const notes = [attachments.length ? 'The attachment was not copied.' : '', password ? 'Set a password again if you need one.' : ''].filter(Boolean);
          goHome({ text, formatter, notice: notes.length ? `Cloned from the paste you were reading. ${notes.join(' ')}` : undefined });
        }),
      ]
    : [];

  // Separators are drawn in CSS so that none is left dangling where the line wraps.
  const metaItems: HTMLElement[] = [
    h('li', { class: 'meta__item', title: 'Decrypted in your browser. The server never sees the content.' }, h('span', { class: 'pill' }, icon('lock', 14, 2.2), 'Decrypted locally')),
  ];
  const addMeta = (value: string, title?: string) => {
    if (value) metaItems.push(h('li', { class: 'meta__item', title }, value));
  };
  addMeta(paste.meta.created ? `Created ${timeAgo(paste.meta.created)}` : '');
  if (burn === 1) addMeta('Burn after reading', 'Deleted from the server when it was opened.');
  else {
    const ttl = paste.meta.time_to_live;
    addMeta(expiresIn(ttl), ttl ? 'After that it is deleted from the server and the link stops working.' : 'Stays until someone deletes it.');
  }
  addMeta(formatterLabel(formatter));

  const content = text ? h('section', { class: 'content', 'aria-label': 'Paste content' }, await renderContent(text, formatter)) : null;
  const discussionSlot = h('div');

  mountPage(
    [
      h('h1', { class: 'sr-only' }, 'Encrypted paste'),
      h('div', { class: 'meta-bar' }, h('div', { class: 'meta' }, h('ul', { class: 'meta__list', 'aria-label': 'About this paste' }, metaItems)), h('div', { class: 'actions' }, actions)),
      burn === 1
        ? h(
            'div',
            { class: 'callout callout--warn', role: 'status' },
            icon('trash', 20),
            h('p', null, h('strong', null, 'This paste has been deleted from the server. '), 'This tab holds the only remaining copy.'),
          )
        : null,
      content,
      attachments.map(renderAttachment),
      discussion === 1 ? discussionSlot : null,
    ],
    { view: true, title: 'Paste' },
  );

  if (discussion === 1) await renderDiscussion(discussionSlot, paste, key, password);
}

// ---------------- Discussion ----------------

interface DecryptedComment {
  id: string;
  parentid: string;
  created?: number;
  nickname: string;
  text: string;
}

function avatarVariant(name: string): string {
  if (!name) return 'c';
  let hash = 0;
  for (const ch of name) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return ['a', 'b', 'd'][hash % 3];
}

/** Comments are text only; anything bigger once decompressed is rejected. */
const MAX_COMMENT_PLAINTEXT = 1024 * 1024;

async function decryptComments(paste: PasteRecord, key: Uint8Array, password: string): Promise<DecryptedComment[]> {
  const results = await Promise.all(
    (paste.comments ?? []).map(async (c: CommentRecord): Promise<DecryptedComment | null> => {
      const adata = c.adata;
      if (!Array.isArray(adata) || typeof c.id !== 'string' || typeof c.parentid !== 'string') return null;
      // Comments created by current clients bind their position in the thread into the AAD.
      if (adata.length === 10 && (adata[8] !== paste.id || adata[9] !== c.parentid)) return null;
      try {
        const m = await decrypt<CommentMessage>(key, password, c.ct, adata, MAX_COMMENT_PLAINTEXT);
        const nickname = typeof m.nickname === 'string' ? m.nickname.trim() : '';
        const text = typeof m.comment === 'string' ? m.comment : '';
        const created = typeof c.meta?.created === 'number' ? c.meta.created : undefined;
        return { id: c.id, parentid: c.parentid, created, nickname, text };
      } catch {
        return null;
      }
    }),
  );
  return results.filter((c): c is DecryptedComment => c !== null).sort((a, b) => (a.created ?? 0) - (b.created ?? 0));
}

interface DiscussionState {
  /** Comment to highlight and focus (the one just posted). */
  highlight?: string;
  /** Nickname typed before the form was rebuilt. */
  nickname?: string;
}

async function renderDiscussion(slot: HTMLElement, paste: PasteRecord, key: Uint8Array, password: string, state: DiscussionState = {}): Promise<void> {
  const comments = await decryptComments(paste, key, password);
  const byParent = new Map<string, DecryptedComment[]>();
  const ids = new Set(comments.map((c) => c.id));
  for (const c of comments) {
    const parent = ids.has(c.parentid) ? c.parentid : paste.id;
    byParent.set(parent, [...(byParent.get(parent) ?? []), c]);
  }

  let replyTo: DecryptedComment | null = null;
  const articles = new Map<string, HTMLElement>();
  const replying = h('div', { class: 'replying', hidden: true });
  const nickname = h('input', { id: 'nick', type: 'text', class: 'input', placeholder: 'Anonymous', maxlength: '64', autocomplete: 'nickname' });
  nickname.value = state.nickname ?? '';
  const body = h('textarea', { id: 'comment', class: 'textarea', rows: '3', placeholder: 'Write a comment…' });
  const bodyLabel = h('label', { for: 'comment', class: 'label' }, 'Comment');
  const formError = h('div', { class: 'callout callout--danger', role: 'alert', hidden: true });
  const post = h('button', { type: 'submit', class: 'btn btn--primary' }, 'Post comment');

  const setReply = (c: DecryptedComment | null) => {
    if (replyTo) articles.get(replyTo.id)?.classList.remove('comment--replying');
    replyTo = c;
    replying.hidden = !c;
    body.placeholder = c ? `Reply to ${c.nickname || 'Anonymous'}…` : 'Write a comment…';
    bodyLabel.textContent = c ? 'Reply' : 'Comment';
    if (c) {
      articles.get(c.id)?.classList.add('comment--replying');
      const cancel = h('button', { type: 'button', class: 'btn btn--ghost' }, 'Cancel');
      cancel.addEventListener('click', () => {
        const target = articles.get(c.id)?.querySelector<HTMLElement>('.comment__reply');
        setReply(null);
        target?.focus();
      });
      replying.replaceChildren(h('span', null, `Replying to ${c.nickname || 'Anonymous'}`), cancel);
      body.focus();
    }
  };

  const renderThread = (parent: string): HTMLElement[] =>
    (byParent.get(parent) ?? []).map((c) => {
      const name = c.nickname || 'Anonymous';
      const reply = h('button', { type: 'button', class: 'btn btn--ghost comment__reply', 'aria-label': `Reply to ${name}` }, 'Reply');
      reply.addEventListener('click', () => setReply(c));
      const children = renderThread(c.id);
      const article = h(
        'article',
        {
          class: `comment${c.id === state.highlight ? ' comment--highlight' : ''}`,
          id: `comment-${c.id}`,
          'aria-label': `Comment by ${name}`,
          tabindex: c.id === state.highlight ? '-1' : null,
        },
        h('span', { class: `avatar avatar--${avatarVariant(c.nickname)}`, 'aria-hidden': 'true' }, c.nickname ? [...c.nickname][0].toUpperCase() : '?'),
        h(
          'div',
          { class: 'comment__body' },
          h('div', { class: 'comment__head' }, h('span', { class: 'comment__name' }, name), h('span', { class: 'small muted' }, timeAgo(c.created))),
          h('p', { class: 'comment__text' }, c.text),
          h('div', { class: 'row' }, reply),
        ),
      );
      articles.set(c.id, article);
      return children.length ? h('div', null, article, h('div', { class: 'comment-children' }, children)) : article;
    });

  const thread = renderThread(paste.id);
  const form = h(
    'form',
    { class: 'card comment-form', novalidate: true },
    replying,
    h('div', { class: 'field comment-form__nick' }, h('label', { for: 'nick', class: 'label' }, 'Nickname ', h('span', { class: 'muted' }, '(optional)')), nickname),
    h('div', { class: 'field' }, bodyLabel, body),
    formError,
    h('div', { class: 'row comment-form__footer' }, post),
  );

  body.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      form.requestSubmit();
    }
  });

  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    formError.hidden = true;
    const text = body.value;
    if (!text.trim()) return body.focus();
    post.setAttribute('aria-busy', 'true');
    try {
      const parentid = replyTo?.id ?? paste.id;
      const message: CommentMessage = { comment: text };
      if (nickname.value.trim()) message.nickname = nickname.value.trim();
      const { ct, adata } = await encryptComment(key, password, message, paste.id, parentid);
      const created = await postComment({ v: 2, adata, ct, pasteid: paste.id, parentid });
      const fresh = await getPaste(paste.id);
      await renderDiscussion(slot, fresh, key, password, { highlight: created.id, nickname: nickname.value });
      const posted = document.getElementById(`comment-${created.id}`);
      posted?.focus({ preventScroll: true });
      posted?.scrollIntoView({ behavior: prefersReducedMotion() ? 'auto' : 'smooth', block: 'center' });
      toast('Comment posted');
    } catch (err) {
      post.removeAttribute('aria-busy');
      formError.replaceChildren(icon('info', 20), h('p', null, err instanceof ApiError ? err.message : 'Could not post the comment.'));
      formError.hidden = false;
    }
  });

  clear(slot);
  slot.appendChild(
    h(
      'section',
      { class: 'discussion', 'aria-labelledby': 'discussion-title' },
      h(
        'div',
        { class: 'row' },
        h('h2', { id: 'discussion-title', class: 'discussion__title' }, 'Discussion'),
        h('span', { class: 'small muted' }, `${comments.length} comment${comments.length === 1 ? '' : 's'} · encrypted with the same key`),
      ),
      thread.length ? h('div', { class: 'card comments' }, thread) : null,
      form,
    ),
  );
}
