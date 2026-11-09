// Centered single-purpose screens: loading, password prompt, burn confirmation, errors.
import { announce, h } from '../dom';
import { icon } from '../icons';
import { goHome } from './create';
import { mountPage } from './layout';

export function showLoading(message: string): void {
  mountPage(h('div', { class: 'gate', 'aria-busy': 'true' }, h('div', { class: 'spinner', 'aria-hidden': 'true' }), h('p', { class: 'gate__text' }, message)), {
    centered: true,
    title: message.replace(/…$/, ''),
  });
  announce(message);
}

export interface PasswordGateOptions {
  /** The previous attempt failed. */
  wrong: boolean;
  /** Burn after reading: the server copy is already gone, only this tab holds the paste. */
  burned: boolean;
}

export function showPasswordGate(options: PasswordGateOptions, onSubmit: (password: string) => void): void {
  const { wrong, burned } = options;
  const input = h('input', {
    id: 'pw',
    type: 'password',
    class: 'input input--lg',
    autocomplete: 'off',
    required: true,
    'aria-invalid': wrong ? 'true' : 'false',
    'aria-describedby': [wrong ? 'pw-error' : '', burned ? 'pw-burned' : ''].filter(Boolean).join(' ') || null,
  });
  const button = h('button', { type: 'submit', class: 'btn btn--primary btn--lg btn--block' }, 'Decrypt');
  const form = h(
    'form',
    { class: 'gate', novalidate: true },
    h('span', { class: 'badge-icon' }, icon('lockKey', 24)),
    h(
      'div',
      { class: 'stack-sm' },
      h('h1', { class: 'gate__title' }, 'This paste is password-protected'),
      h('p', { class: 'gate__text' }, 'Enter the password the sender gave you. It is checked on this device; it is never sent anywhere.'),
    ),
    burned
      ? h(
          'div',
          { id: 'pw-burned', class: 'callout callout--warn' },
          icon('alert', 20),
          h('p', null, h('strong', null, 'Already deleted from the server. '), 'Keep this tab open until you have read it: closing or reloading it loses the paste for good.'),
        )
      : null,
    h(
      'div',
      { class: 'field' },
      h('label', { for: 'pw', class: 'label' }, 'Password'),
      input,
      wrong
        ? h('span', { id: 'pw-error', class: 'field-error' }, icon('info', 14, 2.2), 'Wrong password, or the link is incomplete.')
        : null,
    ),
    button,
  );
  form.addEventListener('submit', (event) => {
    event.preventDefault();
    if (!input.value) return input.focus();
    button.setAttribute('aria-busy', 'true');
    button.textContent = 'Decrypting…';
    onSubmit(input.value);
  });
  mountPage(form, { centered: true, title: 'Password required' });
  input.focus();
  if (wrong) announce('Wrong password, or the link is incomplete.');
}

export function showBurnGate(onOpen: () => void): void {
  const open = h('button', { type: 'button', class: 'btn btn--primary btn--lg' }, 'Open and delete');
  open.addEventListener('click', onOpen);
  const later = h('button', { type: 'button', class: 'btn btn--lg' }, 'Not now');
  later.addEventListener('click', () => goHome());
  mountPage(
    h(
      'section',
      { class: 'gate', 'aria-labelledby': 'burn-title' },
      h('span', { class: 'badge-icon badge-icon--warn' }, icon('flame', 24)),
      h(
        'div',
        { class: 'stack-sm' },
        h('h1', { id: 'burn-title', class: 'gate__title' }, 'This paste can be read only once'),
        h('p', { class: 'gate__text' }, 'As soon as you open it, it is deleted from the server. Copy anything you need before you leave or reload the page.'),
        h('p', { class: 'gate__text' }, 'If the sender set a password, have it ready before you open the paste.'),
      ),
      h('div', { class: 'row' }, open, later),
    ),
    { centered: true, title: 'Read once' },
  );
}

/** Asked before acting on a delete link, so link scanners that run scripts cannot delete pastes. */
export function showDeleteGate(onDelete: () => void): void {
  const remove = h('button', { type: 'button', class: 'btn btn--danger btn--lg' }, 'Delete paste');
  remove.addEventListener('click', onDelete);
  const cancel = h('button', { type: 'button', class: 'btn btn--lg' }, 'Cancel');
  cancel.addEventListener('click', () => goHome());
  mountPage(
    h(
      'section',
      { class: 'gate', 'aria-labelledby': 'delete-title' },
      h('span', { class: 'badge-icon badge-icon--danger' }, icon('trash', 24)),
      h(
        'div',
        { class: 'stack-sm' },
        h('h1', { id: 'delete-title', class: 'gate__title' }, 'Delete this paste?'),
        h('p', { class: 'gate__text' }, 'It will be removed from the server for everyone, together with its comments. This cannot be undone.'),
      ),
      h('div', { class: 'row' }, remove, cancel),
    ),
    { centered: true, title: 'Delete paste' },
  );
}

export type ErrorKind = 'notfound' | 'nokey' | 'badkey' | 'badlink' | 'unreachable' | 'deleted' | 'generic';

const ERRORS: Record<ErrorKind, { eyebrow: string; title: string; body: string; tone: 'danger' | 'warn' | 'accent' }> = {
  notfound: { eyebrow: 'Not found', title: 'This paste is gone', body: 'It does not exist, it has expired, or it was already read and burned.', tone: 'danger' },
  nokey: {
    eyebrow: 'Key missing',
    title: 'This link has no key',
    body: 'The decryption key comes after # in the link. Some apps and URL shorteners cut it off. Ask the sender for the full link.',
    tone: 'danger',
  },
  badkey: { eyebrow: 'Decryption failed', title: 'Could not decrypt this paste', body: 'The key in the link is invalid or incomplete. Ask the sender for the full link, including everything after #.', tone: 'danger' },
  badlink: {
    eyebrow: 'Invalid link',
    title: 'This link looks incomplete',
    body: 'The paste ID in the link is missing or was changed along the way. Ask the sender for the full link.',
    tone: 'danger',
  },
  unreachable: {
    eyebrow: 'Connection problem',
    title: 'Can’t reach the server',
    body: 'The paste was not opened. Check your connection and try again.',
    tone: 'warn',
  },
  deleted: { eyebrow: 'Deleted', title: 'The paste was deleted', body: 'It has been removed from the server and its link no longer works.', tone: 'accent' },
  generic: { eyebrow: 'Error', title: 'Something went wrong', body: 'Please try again.', tone: 'danger' },
};

export interface ErrorOptions {
  detail?: string;
  title?: string;
  /** Offers a "Try again" button instead of "Create a new paste". */
  retry?: () => void;
}

export function showError(kind: ErrorKind, options: ErrorOptions = {}): void {
  const e = ERRORS[kind];
  const badge = e.tone === 'accent' ? 'badge-icon' : `badge-icon badge-icon--${e.tone}`;
  const actions: HTMLButtonElement[] = [];
  if (options.retry) {
    const retry = h('button', { type: 'button', class: 'btn btn--primary btn--lg' }, 'Try again');
    retry.addEventListener('click', options.retry);
    actions.push(retry);
  }
  const create = h('button', { type: 'button', class: options.retry ? 'btn btn--lg' : 'btn btn--primary btn--lg btn--block' }, 'Create a new paste');
  create.addEventListener('click', () => goHome());
  actions.push(create);
  const title = options.title ?? e.title;
  mountPage(
    h(
      'section',
      { class: 'gate', 'aria-labelledby': 'err-title' },
      h('span', { class: badge }, icon(kind === 'deleted' ? 'check' : 'info', 24)),
      h(
        'div',
        { class: 'stack-sm' },
        h('span', { class: 'gate__eyebrow' }, e.eyebrow),
        h('h1', { id: 'err-title', class: 'gate__title' }, title),
        h('p', { class: 'gate__text' }, options.detail ?? e.body),
      ),
      options.retry ? h('div', { class: 'row' }, actions) : actions,
    ),
    { centered: true, title },
  );
}
