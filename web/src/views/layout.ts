// Page chrome shared by every screen: header with theme toggle, main area, footer.
import { baseUrl } from '../api';
import { config } from '../config';
import { Child, clear, h } from '../dom';
import { brandMark, icon } from '../icons';
import { goHome } from './create';

const SOURCE_URL = 'https://github.com/private-bin/privatebin';

function currentTheme(): 'light' | 'dark' {
  return document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
}

function paintThemeButton(button: HTMLElement): void {
  const dark = currentTheme() === 'dark';
  button.replaceChildren(icon(dark ? 'sun' : 'moon', 18, 1.8));
  button.setAttribute('aria-label', dark ? 'Switch to light mode' : 'Switch to dark mode');
}

// theme.ts re-applies the theme when the system preference changes (its listener was
// registered first, so it runs first); keep the toggle's icon and label in sync.
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
  document.querySelectorAll<HTMLElement>('[data-theme-toggle]').forEach(paintThemeButton);
});

function themeButton(): HTMLButtonElement {
  const button = h('button', { type: 'button', class: 'btn btn--icon', 'data-theme-toggle': '' });
  const paint = () => paintThemeButton(button);
  button.addEventListener('click', () => {
    const next = currentTheme() === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = next;
    try {
      localStorage.setItem('pb-theme', next);
    } catch {
      // ignore: preference just won't persist
    }
    paint();
  });
  paint();
  return button;
}

function onHome(event: Event): void {
  const e = event as MouseEvent;
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
  event.preventDefault();
  goHome();
}

export interface PageOptions {
  /** Narrower chrome for centered gate screens. */
  narrow?: boolean;
  /** Content is a centered card rather than a document flow. */
  centered?: boolean;
  title?: string;
}

export function mountPage(content: Child | (Child | Child[])[], options: PageOptions = {}): void {
  const app = document.getElementById('app')!;
  const width = options.narrow ? 'container container--narrow' : 'container';
  const home = baseUrl();

  const header = h(
    'header',
    { class: 'site-header' },
    h(
      'div',
      { class: `${width} site-header__inner` },
      h('a', { href: home, class: 'brand', onclick: onHome }, brandMark(), h('span', null, config.name)),
      h(
        'nav',
        { class: 'site-nav', 'aria-label': 'Main' },
        h('a', { href: home, class: 'btn', onclick: onHome }, icon('plus', 16, 2), 'New paste'),
        themeButton(),
      ),
    ),
  );

  const main = options.centered
    ? h('main', { class: 'center-stage' }, ...(Array.isArray(content) ? content : [content]))
    : h('main', { class: `${width} page${options.title === 'view' ? ' page--view' : ''}` }, ...(Array.isArray(content) ? content : [content]));

  const footer = h(
    'footer',
    { class: `${width} site-footer` },
    h('span', null, 'Zero-knowledge: the server only ever stores ciphertext.'),
    h(
      'span',
      { class: 'site-footer__meta' },
      h('a', { href: SOURCE_URL, target: '_blank', rel: 'noopener noreferrer' }, 'Source on GitHub'),
      config.version ? h('span', null, `v${config.version}`) : null,
    ),
  );

  clear(app);
  app.append(header, main, footer);
  window.scrollTo(0, 0);
}
