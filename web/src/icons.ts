// Inline stroke icons drawn from path data (no external sprite, CSP-friendly).
const SVG_NS = 'http://www.w3.org/2000/svg';

const PATHS: Record<string, string[]> = {
  plus: ['M12 5v14M5 12h14'],
  sun: ['M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8z', 'M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4'],
  moon: ['M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z'],
  lock: ['M7 10.5h10a2.5 2.5 0 0 1 2.5 2.5v5a2.5 2.5 0 0 1-2.5 2.5H7A2.5 2.5 0 0 1 4.5 18v-5A2.5 2.5 0 0 1 7 10.5z', 'M8 10.5V7.5a4 4 0 0 1 8 0v3'],
  lockKey: ['M7 10.5h10a2.5 2.5 0 0 1 2.5 2.5v5a2.5 2.5 0 0 1-2.5 2.5H7A2.5 2.5 0 0 1 4.5 18v-5A2.5 2.5 0 0 1 7 10.5z', 'M8 10.5V7.5a4 4 0 0 1 8 0v3', 'M12 14.5v2'],
  shield: ['M12 3l7 3v5c0 4.5-3 8.3-7 10-4-1.7-7-5.5-7-10V6z', 'M9 12l2 2 4-4'],
  check: ['M5 12.5l4.5 4.5L19 7.5'],
  copy: ['M11 8.5h7a2.5 2.5 0 0 1 2.5 2.5v7a2.5 2.5 0 0 1-2.5 2.5h-7A2.5 2.5 0 0 1 8.5 18v-7A2.5 2.5 0 0 1 11 8.5z', 'M15.5 8.5V6a2.5 2.5 0 0 0-2.5-2.5H6A2.5 2.5 0 0 0 3.5 6v7A2.5 2.5 0 0 0 6 15.5h2.5'],
  qr: ['M3.5 3.5h6.5v6.5H3.5zM14 3.5h6.5v6.5H14zM3.5 14h6.5v6.5H3.5z', 'M14 14h2.5v2.5H14zM18 18h2.5v2.5H18zM18 14h2.5M14 18v2.5'],
  arrowRight: ['M5 12h14M13 6l6 6-6 6'],
  alert: ['M12 3.5L2.5 20h19z', 'M12 10v4.5M12 17.2v.3'],
  info: ['M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z', 'M12 7.5v5.5M12 16.3v.2'],
  paperclip: ['M21 11.5l-8.6 8.6a5.5 5.5 0 0 1-7.8-7.8l8.6-8.6a3.7 3.7 0 0 1 5.2 5.2l-8.6 8.6a1.8 1.8 0 0 1-2.6-2.6l7.9-7.9'],
  image: ['M6 4.5h12a2.5 2.5 0 0 1 2.5 2.5v10a2.5 2.5 0 0 1-2.5 2.5H6A2.5 2.5 0 0 1 3.5 17V7A2.5 2.5 0 0 1 6 4.5z', 'M9 8.2a1.8 1.8 0 1 0 0 3.6a1.8 1.8 0 1 0 0-3.6z', 'M20.5 16l-5-5-8.5 8.5'],
  file: ['M14 3.5H7A2.5 2.5 0 0 0 4.5 6v12A2.5 2.5 0 0 0 7 20.5h10a2.5 2.5 0 0 0 2.5-2.5V9z', 'M14 3.5V9h5.5'],
  download: ['M12 4v11M7 10.5l5 5 5-5M5 19.5h14'],
  x: ['M6 6l12 12M18 6L6 18'],
  flame: ['M12 21c-3.9 0-7-2.9-7-6.6 0-3.1 2.2-5 3.6-7.1.5 1.7 1.5 2.9 2.7 3.4C11.2 7.5 12.6 5 15 3c.1 3.2 4 5.6 4 11.4 0 3.7-3.1 6.6-7 6.6z', 'M12 21c-1.7 0-3-1.3-3-3 0-1.9 1.6-2.8 2.4-4.4.9 1.5 3.6 2.3 3.6 4.4 0 1.7-1.3 3-3 3z'],
  eye: ['M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z', 'M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z'],
  eyeOff: ['M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z', 'M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z', 'M4 4l16 16'],
  refresh: ['M20 11.5A8 8 0 0 0 5.6 6.6L3.5 8.5', 'M3.5 4v4.5H8', 'M4 12.5a8 8 0 0 0 14.4 4.9l2.1-1.9', 'M20.5 20v-4.5H16'],
  trash: ['M4 7h16M9.5 7V4.5h5V7M6 7l1 13h10l1-13'],
};

export function icon(name: keyof typeof PATHS, size = 18, strokeWidth = 1.9): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('width', String(size));
  svg.setAttribute('height', String(size));
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', String(strokeWidth));
  svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  svg.setAttribute('aria-hidden', 'true');
  for (const d of PATHS[name]) {
    const path = document.createElementNS(SVG_NS, 'path');
    path.setAttribute('d', d);
    svg.appendChild(path);
  }
  return svg;
}

export function brandMark(size = 30): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('width', String(size));
  svg.setAttribute('height', String(size));
  svg.setAttribute('viewBox', '0 0 30 30');
  svg.setAttribute('aria-hidden', 'true');
  const rect = document.createElementNS(SVG_NS, 'rect');
  for (const [k, v] of Object.entries({ x: '1', y: '1', width: '28', height: '28', rx: '8', class: 'brand__mark-bg' })) rect.setAttribute(k, v);
  const circle = document.createElementNS(SVG_NS, 'circle');
  for (const [k, v] of Object.entries({ cx: '15', cy: '12.5', r: '3.6', class: 'brand__mark-fg' })) circle.setAttribute(k, v);
  const path = document.createElementNS(SVG_NS, 'path');
  path.setAttribute('d', 'M13.2 15.2 L12.2 21.5 L17.8 21.5 L16.8 15.2 Z');
  path.setAttribute('class', 'brand__mark-fg');
  svg.append(rect, circle, path);
  return svg;
}

export function svgElement(tag: string, attrs: Record<string, string>): SVGElement {
  const el = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  return el;
}
