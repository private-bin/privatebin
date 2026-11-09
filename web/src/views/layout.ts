// Page chrome shared by every screen: header with theme toggle, main area, footer.
import { baseUrl } from '../api';
import { config } from '../config';
import { Child, clear, h } from '../dom';
import { brandMark, icon } from '../icons';
import { goHome } from './create';

const SOURCE_URL = 'https://github.com/private-bin/privatebin';

type ThemeChoice = 'system' | 'light' | 'dark';
const NEXT: Record<ThemeChoice, ThemeChoice> = { system: 'light', light: 'dark', dark: 'system' };
const THEME_ICON = { system: 'monitor', light: 'sun', dark: 'moon' } as const;

function savedTheme(): ThemeChoice {
  try {
    const value = localStorage.getItem('pb-theme');
    return value === 'light' || value === 'dark' ? value : 'system';
  } catch {
    return 'system';
  }
}

function applyTheme(choice: ThemeChoice): void {
  try {
    if (choice === 'system') localStorage.removeItem('pb-theme');
    else localStorage.setItem('pb-theme', choice);
  } catch {
    // storage unavailable: the choice just won't persist
  }
  const systemDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
  document.documentElement.dataset.theme = choice === 'system' ? (systemDark ? 'dark' : 'light') : choice;
}

function paintThemeButton(button: HTMLElement): void {
  const choice = savedTheme();
  const label = `Theme: ${choice === 'system' ? `system (${document.documentElement.dataset.theme ?? 'light'})` : choice}. Switch to ${NEXT[choice]}`;
  button.replaceChildren(icon(THEME_ICON[choice], 18, 1.8));
  button.setAttribute('aria-label', label);
  button.title = label;
}

// theme.ts re-applies the theme when the system preference changes (its listener was
// registered first, so it runs first); keep the toggle's label in sync.
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
  document.querySelectorAll<HTMLElement>('[data-theme-toggle]').forEach(paintThemeButton);
});

function themeButton(): HTMLButtonElement {
  const button = h('button', { type: 'button', class: 'btn btn--icon', 'data-theme-toggle': '' });
  button.addEventListener('click', () => {
    applyTheme(NEXT[savedTheme()]);
    paintThemeButton(button);
  });
  paintThemeButton(button);
  return button;
}

function onHome(event: Event): void {
  const e = event as MouseEvent;
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
  event.preventDefault();
  goHome();
}

export interface PageOptions {
  /** Content is a centered card rather than a document flow. */
  centered?: boolean;
  /** Tighter spacing for the paste viewer. */
  view?: boolean;
  /** Document title, shown as "<title> – <site name>". */
  title?: string;
}

export function mountPage(content: Child | (Child | Child[])[], options: PageOptions = {}): void {
  const app = document.getElementById('app')!;
  const home = baseUrl();
  document.title = options.title ? `${options.title} – ${config.name}` : config.name;

  // Header and footer keep the same width on every screen; only the content narrows.
  const header = h(
    'header',
    { class: 'site-header' },
    h(
      'div',
      { class: 'container site-header__inner' },
      h('a', { href: home, class: 'brand', onclick: onHome }, brandMark(), h('span', null, config.name)),
      h(
        'nav',
        { class: 'site-nav', 'aria-label': 'Main' },
        h('a', { href: home, class: 'btn', onclick: onHome }, icon('plus', 16, 2), 'New paste'),
        themeButton(),
      ),
    ),
  );

  const children = Array.isArray(content) ? content : [content];
  const main = options.centered
    ? h('main', { class: 'center-stage', tabindex: '-1' }, ...children)
    : h('main', { class: `container page${options.view ? ' page--view' : ''}`, tabindex: '-1' }, ...children);

  const footer = h(
    'footer',
    { class: 'container site-footer' },
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
  // Move focus into the new screen so keyboard and screen-reader users start there.
  // Views that focus a control themselves (an input, a button) do so after this.
  const heading = main.querySelector<HTMLElement>('h1');
  if (heading) heading.tabIndex = -1;
  (heading ?? main).focus({ preventScroll: true });
}
