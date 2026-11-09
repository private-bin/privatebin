// "Link ready" screen shown after a paste was uploaded.
import { ApiError, deletePaste } from '../api';
import { announce, copyText, flashButton, h, toast } from '../dom';
import { formatterLabel } from '../format';
import { icon } from '../icons';
import { renderQr } from '../render';
import { showError } from './gates';
import { mountPage } from './layout';
import { showPaste } from './view';

export interface CreatedInfo {
  id: string;
  shareUrl: string;
  deleteUrl: string;
  deleteToken: string;
  expire: string;
  formatter: string;
  burn: boolean;
  discussion: boolean;
  /** Shown once so it can be shared; it never leaves this page. */
  password: string;
}

function splitUrl(info: CreatedInfo): HTMLElement {
  const marker = info.burn ? '#-' : '#';
  const at = info.shareUrl.indexOf(marker);
  const before = info.shareUrl.slice(0, at);
  const idAt = before.length - info.id.length;
  return h(
    'div',
    { class: 'share-url', 'aria-label': 'Share link' },
    h('span', { class: 'share-url__base' }, before.slice(0, idAt)),
    h('span', { class: 'share-url__id' }, info.id),
    h('span', { class: 'share-url__hash' }, marker),
    h('span', { class: 'share-url__key' }, info.shareUrl.slice(at + marker.length)),
  );
}

export function showCreated(info: CreatedInfo): void {
  const copy = h('button', { type: 'button', class: 'btn btn--primary btn--lg' }, icon('copy', 18, 2), 'Copy link');
  copy.addEventListener('click', async () => {
    const ok = await copyText(info.shareUrl);
    flashButton(copy, [icon(ok ? 'check' : 'copy', 18, 2), ok ? 'Copied' : 'Copy failed'], ok ? 'Link copied' : 'Copy failed');
  });

  const qrSlot = h('div', { class: 'qr' });
  const qrCard = h(
    'section',
    { class: 'card card--pad row', 'aria-label': 'QR code', hidden: true },
    qrSlot,
    h(
      'div',
      { class: 'stack-xs' },
      h('strong', null, 'Scan to open on another device'),
      h('span', { class: 'small muted' }, 'The QR code contains the full link, key included. Treat it like the link itself.'),
    ),
  );
  const qrToggle = h('button', { type: 'button', class: 'btn btn--lg', 'aria-pressed': 'false' }, icon('qr', 18, 1.8), 'Show QR code');
  qrToggle.addEventListener('click', async () => {
    const show = qrCard.hidden;
    if (show && !qrSlot.firstChild) qrSlot.appendChild(await renderQr(info.shareUrl));
    qrCard.hidden = !show;
    qrToggle.setAttribute('aria-pressed', String(show));
    qrToggle.replaceChildren(icon('qr', 18, 1.8), show ? 'Hide QR code' : 'Show QR code');
  });

  const open = h('button', { type: 'button', class: 'btn btn--lg' }, 'Open paste', icon('arrowRight', 16, 2));
  open.addEventListener('click', () => void showPaste(info.id));

  const copyDelete = h('button', { type: 'button', class: 'btn' }, 'Copy');
  copyDelete.addEventListener('click', async () => {
    const ok = await copyText(info.deleteUrl);
    flashButton(copyDelete, [ok ? 'Copied' : 'Copy failed'], ok ? 'Delete link copied' : 'Copy failed');
  });

  // Two-step delete: the first click arms the button for a few seconds.
  let armTimer: number | undefined;
  const deleteNow = h('button', { type: 'button', class: 'btn btn--danger' }, 'Delete now');
  const disarm = () => {
    window.clearTimeout(armTimer);
    armTimer = undefined;
    deleteNow.textContent = 'Delete now';
    deleteNow.classList.remove('btn--armed');
  };
  deleteNow.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && armTimer !== undefined) disarm();
  });
  deleteNow.addEventListener('blur', () => {
    if (armTimer !== undefined) disarm();
  });
  deleteNow.addEventListener('click', async () => {
    if (armTimer === undefined) {
      deleteNow.textContent = 'Click again to delete';
      deleteNow.classList.add('btn--armed');
      announce('Click again to delete the paste.');
      armTimer = window.setTimeout(disarm, 4000);
      return;
    }
    window.clearTimeout(armTimer);
    deleteNow.setAttribute('aria-busy', 'true');
    try {
      await deletePaste(info.id, info.deleteToken);
      history.replaceState(null, '', info.deleteUrl.split('?')[0]);
      showError('deleted');
    } catch (err) {
      deleteNow.removeAttribute('aria-busy');
      disarm();
      toast(err instanceof ApiError ? err.message : 'Could not delete the paste.');
    }
  });

  let passwordCard: HTMLElement | null = null;
  if (info.password) {
    const copyPassword = h('button', { type: 'button', class: 'btn' }, icon('copy', 16, 2), 'Copy');
    copyPassword.addEventListener('click', async () => {
      const ok = await copyText(info.password);
      flashButton(copyPassword, [icon(ok ? 'check' : 'copy', 16, 2), ok ? 'Copied' : 'Copy failed'], ok ? 'Password copied' : 'Copy failed');
    });
    passwordCard = h(
      'section',
      { class: 'card card--pad row row--between', 'aria-label': 'Password' },
      h(
        'div',
        { class: 'stack-xs flex-fill' },
        h('strong', null, 'Password'),
        h('span', { class: 'secret-value' }, info.password),
        h('span', { class: 'small muted' }, 'Send it through a different channel than the link. It is not stored anywhere.'),
      ),
      copyPassword,
    );
  }

  const detail = (term: string, value: string) => [h('dt', null, term), h('dd', null, value)];

  mountPage([
    h(
      'div',
      { class: 'row' },
      h('span', { class: 'badge-icon badge-icon--sm' }, icon('check', 22, 2.2)),
      h(
        'div',
        { class: 'stack-xs' },
        h('h1', { class: 'page-title' }, 'Your link is ready'),
        h('p', { class: 'page-lede' }, info.burn ? 'It can be opened exactly once, then it is deleted.' : 'Anyone with the full link can read this paste.'),
      ),
    ),
    h(
      'section',
      { class: 'card card--pad stack-sm', 'aria-label': 'Share link' },
      splitUrl(info),
      h(
        'div',
        { class: 'legend' },
        h('span', null, h('i', { 'aria-hidden': 'true' }), 'Paste ID: sent to the server'),
        h('span', null, h('i', { class: 'legend__key', 'aria-hidden': 'true' }), 'Key after #: never leaves your browser'),
      ),
      h('div', { class: 'row' }, copy, qrToggle, info.burn ? null : open),
    ),
    passwordCard,
    h(
      'div',
      { class: 'callout callout--warn' },
      icon('alert', 20),
      h('p', null, h('strong', null, 'Save this link now. '), 'The key is not stored anywhere else. If you lose it, nobody can decrypt this paste — not even the server owner.'),
    ),
    h(
      'div',
      { class: 'split' },
      qrCard,
      h(
        'section',
        { class: 'card card--pad stack-sm', 'aria-label': 'Paste details' },
        h('strong', null, 'Details'),
        h(
          'dl',
          { class: 'details' },
          detail(
            'Expires',
            info.burn
              ? info.expire === 'Never'
                ? 'After first read'
                : `After first read, or in ${info.expire} if never opened`
              : info.expire === 'Never'
                ? 'Never'
                : `in ${info.expire}`,
          ),
          detail('Format', formatterLabel(info.formatter)),
          detail('Burn after reading', info.burn ? 'On' : 'Off'),
          detail('Discussion', info.discussion ? 'Open' : 'Closed'),
          detail('Password', info.password ? 'Yes' : 'None'),
        ),
      ),
    ),
    h(
      'section',
      { class: 'card card--pad row row--between', 'aria-label': 'Delete link' },
      h(
        'div',
        { class: 'stack-xs flex-fill' },
        h('strong', null, 'Delete link'),
        h('span', { class: 'small muted' }, 'Anyone with this link can delete the paste. Keep it to yourself.'),
        h('span', { class: 'small muted mono truncate' }, info.deleteUrl),
      ),
      h('div', { class: 'row' }, copyDelete, deleteNow),
    ),
  ], { title: 'Link ready' });
  copy.focus();
}
