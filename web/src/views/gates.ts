// Centered single-purpose screens: loading, password prompt, burn confirmation, errors.
import { h } from '../dom';
import { icon } from '../icons';
import { goHome } from './create';
import { mountPage } from './layout';

export function showLoading(message: string): void {
  mountPage(h('div', { class: 'gate', 'aria-busy': 'true' }, h('div', { class: 'spinner', 'aria-hidden': 'true' }), h('p', { class: 'gate__text', role: 'status' }, message)), {
    narrow: true,
    centered: true,
  });
}

export function showPasswordGate(wrong: boolean, onSubmit: (password: string) => void): void {
  const input = h('input', {
    id: 'pw',
    type: 'password',
    class: 'input input--lg',
    autocomplete: 'off',
    required: true,
    'aria-invalid': wrong ? 'true' : 'false',
    'aria-describedby': wrong ? 'pw-error' : null,
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
  mountPage(form, { narrow: true, centered: true });
  input.focus();
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
      ),
      h('div', { class: 'row' }, open, later),
    ),
    { narrow: true, centered: true },
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
    { narrow: true, centered: true },
  );
}

export type ErrorKind = 'notfound' | 'nokey' | 'badkey' | 'deleted' | 'generic';

const ERRORS: Record<ErrorKind, { eyebrow: string; title: string; body: string; tone: 'danger' | 'warn' | 'accent' }> = {
  notfound: { eyebrow: 'Not found', title: 'This paste is gone', body: 'It does not exist, it has expired, or it was already read and burned.', tone: 'danger' },
  nokey: {
    eyebrow: 'Key missing',
    title: 'This link has no key',
    body: 'The decryption key comes after # in the link. Some apps and URL shorteners cut it off. Ask the sender for the full link.',
    tone: 'danger',
  },
  badkey: { eyebrow: 'Decryption failed', title: 'Could not decrypt this paste', body: 'The key in the link is invalid or incomplete. Ask the sender for the full link, including everything after #.', tone: 'danger' },
  deleted: { eyebrow: 'Deleted', title: 'The paste was deleted', body: 'It has been removed from the server and its link no longer works.', tone: 'accent' },
  generic: { eyebrow: 'Error', title: 'Something went wrong', body: 'Please try again.', tone: 'danger' },
};

export function showError(kind: ErrorKind, detail?: string, title?: string): void {
  const e = ERRORS[kind];
  const badge = e.tone === 'accent' ? 'badge-icon' : `badge-icon badge-icon--${e.tone}`;
  const action = h('button', { type: 'button', class: 'btn btn--primary btn--lg btn--block' }, 'Create a new paste');
  action.addEventListener('click', () => goHome());
  mountPage(
    h(
      'section',
      { class: 'gate', 'aria-labelledby': 'err-title' },
      h('span', { class: badge }, icon(kind === 'deleted' ? 'check' : 'info', 24)),
      h(
        'div',
        { class: 'stack-sm' },
        h('span', { class: 'gate__eyebrow' }, e.eyebrow),
        h('h1', { id: 'err-title', class: 'gate__title' }, title ?? e.title),
        h('p', { class: 'gate__text' }, detail ?? e.body),
      ),
      action,
    ),
    { narrow: true, centered: true },
  );
}
