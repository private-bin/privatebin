// Human-readable formatting helpers.

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB'];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

export function formatMiB(bytes: number): string {
  const mib = bytes / (1024 * 1024);
  return `${Number.isInteger(mib) ? mib : mib.toFixed(1)} MiB`;
}

function span(seconds: number): [number, string] {
  const steps: [number, string][] = [
    [31_536_000, 'year'],
    [2_592_000, 'month'],
    [604_800, 'week'],
    [86_400, 'day'],
    [3_600, 'hour'],
    [60, 'minute'],
  ];
  for (const [size, name] of steps) {
    if (seconds >= size) return [Math.floor(seconds / size), name];
  }
  return [Math.max(1, Math.floor(seconds)), 'second'];
}

const plural = (n: number, unit: string) => `${n} ${unit}${n === 1 ? '' : 's'}`;

export function timeAgo(unixSeconds: number | undefined): string {
  if (!unixSeconds) return '';
  const diff = Date.now() / 1000 - unixSeconds;
  if (diff < 45) return 'just now';
  const [n, unit] = span(diff);
  return `${plural(n, unit)} ago`;
}

export function expiresIn(ttlSeconds: number | undefined): string {
  if (!ttlSeconds || ttlSeconds <= 0) return 'Never expires';
  // Round up by 1% (at most an hour) so a fresh 1-week paste reads "1 week", not "6 days",
  // while a fresh 5-minute paste still reads "5 minutes".
  const [n, unit] = span(ttlSeconds + Math.min(3600, ttlSeconds * 0.01));
  return `Expires in ${plural(n, unit)}`;
}

export function formatterLabel(formatter: string): string {
  switch (formatter) {
    case 'syntaxhighlighting':
      return 'Code';
    case 'markdown':
      return 'Markdown';
    default:
      return 'Plain text';
  }
}
