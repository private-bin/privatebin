// "New paste" screen: editor, options, client-side encryption and upload.
import { ApiError, baseUrl, createPaste } from '../api';
import { base58Encode } from '../base58';
import { config, expireLabel } from '../config';
import { encryptPaste, generatePassword, randomBytes } from '../crypto';
import { clear, h } from '../dom';
import { formatBytes, formatMiB } from '../format';
import { icon } from '../icons';
import { renderContent } from '../render';
import { showCreated } from './created';
import { mountPage } from './layout';

export interface CreateInit {
  text?: string;
  formatter?: string;
  /** Shown above the editor, e.g. what a clone did not carry over. */
  notice?: string;
}

/** Arrow-key navigation for a group of tabs or radio buttons: one Tab stop, selection follows focus. */
function rovingGroup(buttons: HTMLButtonElement[], isSelected: (i: number) => boolean, select: (i: number) => void): () => void {
  const paint = () => buttons.forEach((b, i) => (b.tabIndex = isSelected(i) ? 0 : -1));
  buttons.forEach((b, i) =>
    b.addEventListener('keydown', (e) => {
      const last = buttons.length - 1;
      const next = { ArrowRight: i === last ? 0 : i + 1, ArrowDown: i === last ? 0 : i + 1, ArrowLeft: i === 0 ? last : i - 1, ArrowUp: i === 0 ? last : i - 1, Home: 0, End: last }[e.key];
      if (next === undefined) return;
      e.preventDefault();
      select(next);
      paint();
      buttons[next].focus();
    }),
  );
  paint();
  return paint;
}

const FORMATS: { value: string; label: string }[] = [
  { value: 'plaintext', label: 'Plain text' },
  { value: 'syntaxhighlighting', label: 'Code' },
  { value: 'markdown', label: 'Markdown' },
];

/** Navigates to the editor without a page reload. */
export function goHome(init: CreateInit = {}): void {
  history.pushState(null, '', baseUrl());
  showCreate(init);
}

function readAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

function switchButton(labelId: string, checked: boolean): HTMLButtonElement {
  return h('button', { type: 'button', role: 'switch', class: 'switch', 'aria-checked': String(checked), 'aria-labelledby': labelId });
}

export function showCreate(init: CreateInit = {}): void {
  const state = {
    tab: 'write' as 'write' | 'preview',
    formatter: FORMATS.some((f) => f.value === init.formatter) ? init.formatter! : config.defaultFormatter,
    burn: config.burnSelected,
    discussion: false,
    file: null as File | null,
    busy: false,
  };

  // ---- Editor ----
  const textarea = h('textarea', {
    id: 'paste-text',
    class: 'editor__text',
    placeholder: 'Paste or type your text…',
    spellcheck: 'false',
    autocomplete: 'off',
  });
  textarea.value = init.text ?? '';
  const preview = h('div', { id: 'paste-preview', class: 'editor__preview', role: 'tabpanel', 'aria-labelledby': 'tab-preview', tabindex: '0', hidden: true });

  const TABS = ['write', 'preview'] as const;
  const tabButtons = TABS.map((tab) => {
    const b = h(
      'button',
      { type: 'button', role: 'tab', id: `tab-${tab}`, 'aria-controls': tab === 'write' ? 'paste-text' : 'paste-preview', 'aria-selected': String(state.tab === tab) },
      tab === 'write' ? 'Write' : 'Preview',
    );
    b.addEventListener('click', () => {
      setTab(tab);
      paintTabs();
    });
    return b;
  });
  const paintTabs = rovingGroup(tabButtons, (i) => TABS[i] === state.tab, (i) => setTab(TABS[i], false));

  function setFormatter(value: string): void {
    state.formatter = value;
    formatButtons.forEach((other, i) => other.setAttribute('aria-checked', String(FORMATS[i].value === value)));
    if (state.tab === 'preview') void renderPreview();
  }
  const formatButtons = FORMATS.map((f) => {
    const b = h('button', { type: 'button', role: 'radio', 'aria-checked': String(state.formatter === f.value) }, f.label);
    b.addEventListener('click', () => {
      setFormatter(f.value);
      paintFormats();
    });
    return b;
  });
  const paintFormats = rovingGroup(formatButtons, (i) => FORMATS[i].value === state.formatter, (i) => setFormatter(FORMATS[i].value));

  async function renderPreview(): Promise<void> {
    clear(preview);
    const text = textarea.value;
    if (!text.trim()) {
      preview.appendChild(h('div', { class: 'editor__preview-empty' }, 'Nothing to preview yet. Write something first.'));
      return;
    }
    preview.appendChild(await renderContent(text, state.formatter));
  }

  function setTab(tab: 'write' | 'preview', focusEditor = true): void {
    state.tab = tab;
    tabButtons.forEach((b, i) => b.setAttribute('aria-selected', String(TABS[i] === tab)));
    textarea.hidden = tab !== 'write';
    preview.hidden = tab !== 'preview';
    if (tab === 'preview') void renderPreview();
    else if (focusEditor) textarea.focus();
  }

  // ---- Attachment ----
  const fileInput = h('input', { type: 'file', class: 'sr-only', tabindex: '-1', 'aria-hidden': 'true' });
  const dropzone = h('div', { class: 'dropzone' });

  function paintDropzone(): void {
    clear(dropzone);
    if (state.file) {
      const remove = h('button', { type: 'button', class: 'btn btn--sm', 'aria-label': `Remove ${state.file.name}` }, icon('x', 14, 2), 'Remove');
      remove.addEventListener('click', () => setFile(null));
      dropzone.append(
        h('div', { class: 'dropzone__file' }, icon('paperclip', 18, 1.8), h('span', null, state.file.name), h('span', { class: 'muted small' }, formatBytes(state.file.size))),
        remove,
      );
      return;
    }
    const browse = h('button', { type: 'button', class: 'btn btn--sm' }, 'Browse…');
    browse.addEventListener('click', () => fileInput.click());
    dropzone.append(
      h(
        'div',
        { class: 'row' },
        h('span', { class: 'muted' }, icon('paperclip', 18, 1.8)),
        h('span', { class: 'small' }, h('strong', null, 'Attach a file')),
        h('span', { class: 'small muted hide-sm' }, 'Drop it here, paste an image, or'),
        browse,
      ),
      h('span', { class: 'small muted' }, `Encrypted too · up to ${formatMiB(config.sizeLimit)}`),
    );
  }

  function setFile(file: File | null): void {
    state.file = file;
    fileInput.value = '';
    hideError();
    if (file && file.size > config.sizeLimit) {
      showError(`“${file.name}” is ${formatBytes(file.size)}. Pastes are limited to ${formatMiB(config.sizeLimit)}, including attachments.`);
    }
    paintDropzone();
  }

  fileInput.addEventListener('change', () => setFile(fileInput.files?.[0] ?? null));

  // ---- Options ----
  const expire = h(
    'select',
    { id: 'expire', class: 'select' },
    config.expireOptions.map((o) => h('option', { value: o.value, selected: o.value === config.expireDefault }, o.label)),
  );
  const password = h('input', {
    id: 'password',
    type: 'password',
    class: 'input input--secret',
    placeholder: 'Optional',
    autocomplete: 'new-password',
    autocapitalize: 'off',
    spellcheck: 'false',
  });
  const reveal = h('button', { type: 'button', class: 'input-group__btn', 'aria-pressed': 'false' });
  const generate = h(
    'button',
    { type: 'button', class: 'input-group__btn', 'aria-label': 'Generate a random password', title: 'Generate a random password' },
    icon('refresh', 18),
  );
  function setRevealed(on: boolean): void {
    password.type = on ? 'text' : 'password';
    const label = on ? 'Hide password' : 'Show password';
    reveal.setAttribute('aria-pressed', String(on));
    reveal.setAttribute('aria-label', label);
    reveal.title = label;
    reveal.replaceChildren(icon(on ? 'eyeOff' : 'eye', 18));
  }
  setRevealed(false);
  reveal.addEventListener('click', () => setRevealed(password.type === 'password'));
  generate.addEventListener('click', () => {
    password.value = generatePassword();
    setRevealed(true);
    password.focus();
  });

  const burnSwitch = switchButton('burn-label', state.burn);
  const discussionSwitch = switchButton('disc-label', false);
  const discussionHint = h('span', { class: 'option__hint' });
  const discussionCard = h(
    'div',
    { class: 'option option--toggle' },
    h('div', { class: 'stack-xs' }, h('span', { id: 'disc-label', class: 'label' }, 'Open discussion'), discussionHint),
    discussionSwitch,
  );

  function paintSwitches(): void {
    burnSwitch.setAttribute('aria-checked', String(state.burn));
    discussionSwitch.setAttribute('aria-checked', String(state.discussion && !state.burn));
    discussionSwitch.disabled = state.burn;
    discussionCard.classList.toggle('option--disabled', state.burn);
    discussionHint.textContent = state.burn ? 'Not available with burn after reading.' : 'Readers can leave encrypted comments.';
  }
  burnSwitch.addEventListener('click', () => {
    state.burn = !state.burn;
    paintSwitches();
  });
  discussionSwitch.addEventListener('click', () => {
    if (state.burn) return;
    state.discussion = !state.discussion;
    paintSwitches();
  });

  // ---- Errors & submit ----
  const errorBox = h('div', { class: 'callout callout--danger', role: 'alert', hidden: true });
  function showError(message: string): void {
    errorBox.replaceChildren(icon('info', 20), h('p', null, message));
    errorBox.hidden = false;
  }
  function hideError(): void {
    errorBox.hidden = true;
  }

  const submit = h('button', { type: 'button', class: 'btn btn--primary btn--lg' }, icon('lock', 18, 2), 'Encrypt & create link');
  submit.addEventListener('click', () => void send());

  async function send(): Promise<void> {
    if (state.busy) return;
    hideError();
    const text = textarea.value;
    const file = state.file;
    if (!text.trim() && !file) {
      showError('Write something or attach a file first.');
      textarea.focus();
      return;
    }
    if (file && file.size > config.sizeLimit) {
      showError(`Pastes are limited to ${formatMiB(config.sizeLimit)}, including attachments. Remove the attachment or split the text.`);
      return;
    }

    state.busy = true;
    submit.setAttribute('aria-busy', 'true');
    submit.replaceChildren(icon('lock', 18, 2), 'Encrypting…');
    try {
      const key = randomBytes(32);
      const message: Record<string, unknown> = { paste: text };
      if (file) {
        message.attachment = [await readAsDataUrl(file)];
        message.attachment_name = [file.name];
      }
      const burn = state.burn;
      const discussion = config.discussion && state.discussion && !burn;
      const { ct, adata } = await encryptPaste(key, password.value, message, state.formatter, discussion, burn);
      if (ct.length > config.sizeLimit) {
        throw new ApiError(`This paste is over ${formatMiB(config.sizeLimit)} once encrypted. Remove the attachment or split the text.`);
      }
      const created = await createPaste({ v: 2, adata, ct, meta: { expire: expire.value } });
      // Both end up in URLs: never trust their shape.
      if (!/^[0-9a-f]{16}$/.test(created.id) || !/^[0-9a-f]{16,128}$/.test(created.deletetoken)) {
        throw new ApiError('The server sent an invalid response.');
      }
      const keyText = base58Encode(key);
      const shareUrl = `${baseUrl()}?${created.id}${burn ? '#-' : '#'}${keyText}`;
      history.pushState(null, '', shareUrl);
      showCreated({
        id: created.id,
        shareUrl,
        deleteToken: created.deletetoken,
        deleteUrl: `${baseUrl()}?pasteid=${created.id}&deletetoken=${created.deletetoken}`,
        expire: expireLabel(expire.value),
        formatter: state.formatter,
        burn,
        discussion,
        password: password.value,
      });
    } catch (err) {
      state.busy = false;
      submit.removeAttribute('aria-busy');
      submit.replaceChildren(icon('lock', 18, 2), 'Encrypt & create link');
      showError(err instanceof ApiError ? err.message : 'Encryption failed in this browser. Make sure the page is served over HTTPS.');
    }
  }

  // ---- Assemble ----
  const editor = h(
    'section',
    { class: 'card card--clip', 'aria-label': 'Editor' },
    h(
      'div',
      { class: 'editor__bar' },
      h('div', { class: 'segmented', role: 'tablist', 'aria-label': 'Editor mode' }, tabButtons),
      h(
        'div',
        { class: 'row' },
        h('span', { id: 'fmt-label', class: 'small muted' }, 'Format'),
        h('div', { class: 'segmented', role: 'radiogroup', 'aria-labelledby': 'fmt-label' }, formatButtons),
      ),
    ),
    h('label', { for: 'paste-text', class: 'sr-only' }, 'Paste content'),
    textarea,
    preview,
    config.fileUpload ? [fileInput, dropzone] : null,
  );

  if (config.fileUpload) {
    let depth = 0;
    editor.addEventListener('dragenter', (e) => {
      if (!e.dataTransfer?.types.includes('Files')) return;
      e.preventDefault();
      depth++;
      dropzone.classList.add('dropzone--active');
    });
    editor.addEventListener('dragover', (e) => {
      if (e.dataTransfer?.types.includes('Files')) e.preventDefault();
    });
    editor.addEventListener('dragleave', () => {
      depth = Math.max(0, depth - 1);
      if (depth === 0) dropzone.classList.remove('dropzone--active');
    });
    editor.addEventListener('drop', (e) => {
      const file = e.dataTransfer?.files?.[0];
      depth = 0;
      dropzone.classList.remove('dropzone--active');
      if (!file) return;
      e.preventDefault();
      setFile(file);
    });
    textarea.addEventListener('paste', (e) => {
      const file = e.clipboardData?.files?.[0];
      if (file) {
        e.preventDefault();
        setFile(file);
      }
    });
    paintDropzone();
  }


  const options = h(
    'section',
    { class: 'options', 'aria-label': 'Options' },
    h('div', { class: 'option' }, h('label', { for: 'expire', class: 'label' }, 'Expires'), expire),
    config.password
      ? h(
          'div',
          { class: 'option' },
          h('label', { for: 'password', class: 'label' }, 'Password'),
          h('div', { class: 'input-group' }, password, h('div', { class: 'input-group__actions' }, reveal, generate)),
        )
      : null,
    h(
      'div',
      { class: 'option option--toggle' },
      h('div', { class: 'stack-xs' }, h('span', { id: 'burn-label', class: 'label' }, 'Burn after reading'), h('span', { class: 'option__hint' }, 'Deleted the first time it is opened.')),
      burnSwitch,
    ),
    config.discussion ? discussionCard : null,
  );
  paintSwitches();

  // Ctrl/⌘ + Enter creates the paste from anywhere in the form, not only the editor.
  const onShortcut = (e: KeyboardEvent) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void send();
    }
  };
  editor.addEventListener('keydown', onShortcut);
  options.addEventListener('keydown', onShortcut);

  const notice = init.notice ? h('div', { class: 'callout callout--info', role: 'status' }, icon('info', 20), h('p', null, init.notice)) : null;

  const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
  mountPage([
    h(
      'div',
      { class: 'stack-sm' },
      h('h1', { class: 'page-title' }, 'New paste'),
      h('p', { class: 'page-lede' }, 'Encrypted in your browser before it is sent. The key never reaches the server.'),
    ),
    notice,
    editor,
    options,
    errorBox,
    h(
      'div',
      { class: 'submit-row' },
      h('p', { class: 'trust-note' }, icon('shield', 18, 1.8), h('span', null, 'AES-256-GCM, key generated on this device.')),
      h('div', { class: 'row' }, h('span', { class: 'kbd hide-sm' }, isMac ? '⌘ + Enter' : 'Ctrl + Enter'), submit),
    ),
  ]);
  textarea.focus();
}
