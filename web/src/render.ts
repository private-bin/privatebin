// Renders decrypted paste content. Heavy libraries are loaded on demand.
import { h } from './dom';

const MAX_HIGHLIGHT_BYTES = 256 * 1024;

export function renderPlain(text: string): HTMLElement {
  return h('div', { class: 'content__scroll' }, h('pre', { class: 'content__plain' }, text));
}

/** Splits highlight.js HTML into lines, re-opening spans that cross line breaks. */
function splitHighlighted(html: string): string[] {
  const lines: string[] = [];
  const open: string[] = [];
  let current = '';
  const re = /(<span[^>]*>)|(<\/span>)|(\n)|([^<\n]+|<)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(html)) !== null) {
    if (m[1]) {
      open.push(m[1]);
      current += m[1];
    } else if (m[2]) {
      open.pop();
      current += m[2];
    } else if (m[3]) {
      lines.push(current + '</span>'.repeat(open.length));
      current = open.join('');
    } else {
      current += m[4];
    }
  }
  lines.push(current + '</span>'.repeat(open.length));
  return lines;
}

export async function renderCode(source: string): Promise<HTMLElement> {
  const text = source.replace(/\r?\n$/, '');
  const code = h('div', { class: 'code' });
  let lines: string[];
  if (new Blob([text]).size <= MAX_HIGHLIGHT_BYTES) {
    const [{ default: hljs }, { default: DOMPurify }] = await Promise.all([import('highlight.js/lib/common'), import('dompurify')]);
    const html = hljs.highlightAuto(text).value;
    lines = splitHighlighted(html).map((line) => DOMPurify.sanitize(line, { ALLOWED_TAGS: ['span'], ALLOWED_ATTR: ['class'] }));
  } else {
    lines = text.split('\n').map((line) => line.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'));
  }
  for (const line of lines) {
    const span = h('span', { class: 'code__text' });
    span.innerHTML = line || ' ';
    code.appendChild(h('div', { class: 'code__line' }, span));
  }
  return h('div', { class: 'content__scroll' }, code);
}

let linkHookInstalled = false;

export async function renderMarkdown(text: string): Promise<HTMLElement> {
  const [{ marked }, { default: DOMPurify }] = await Promise.all([import('marked'), import('dompurify')]);
  if (!linkHookInstalled) {
    linkHookInstalled = true;
    DOMPurify.addHook('afterSanitizeAttributes', (node) => {
      if (node.tagName === 'A') {
        node.setAttribute('target', '_blank');
        node.setAttribute('rel', 'noopener noreferrer nofollow');
      }
    });
  }
  const html = await marked.parse(text, { gfm: true, breaks: false });
  const el = h('div', { class: 'markdown' });
  el.innerHTML = DOMPurify.sanitize(html, {
    // HTML only: SVG/MathML links escape the anchor hook and navigate the app itself.
    USE_PROFILES: { html: true },
    FORBID_TAGS: ['style', 'form', 'input', 'button', 'textarea', 'select', 'option', 'dialog'],
    // No styling hooks or ids: the paste must not reuse the app's classes to fake its UI
    // (overlays, toasts, gates) or shadow element ids the app looks up.
    FORBID_ATTR: ['style', 'class', 'id', 'name'],
  });
  return el;
}

export async function renderContent(text: string, formatter: string): Promise<HTMLElement> {
  if (formatter === 'syntaxhighlighting') return renderCode(text);
  if (formatter === 'markdown') return renderMarkdown(text);
  return renderPlain(text);
}

/** Builds a QR code for the share link as inline SVG rects. */
export async function renderQr(text: string): Promise<SVGSVGElement> {
  const { default: qrcode } = await import('qrcode-generator');
  const qr = qrcode(0, 'M');
  qr.addData(text);
  qr.make();
  const n = qr.getModuleCount();
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', `0 0 ${n} ${n}`);
  svg.setAttribute('shape-rendering', 'crispEdges');
  svg.setAttribute('role', 'img');
  svg.setAttribute('aria-label', 'QR code for the share link');
  let d = '';
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      if (qr.isDark(r, c)) d += `M${c} ${r}h1v1h-1z`;
    }
  }
  const path = document.createElementNS(ns, 'path');
  path.setAttribute('d', d);
  path.setAttribute('fill', '#11151B');
  svg.appendChild(path);
  return svg;
}
