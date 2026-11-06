// Public server configuration, injected by the server into <script id="pb-config">.

export interface ExpireOption {
  value: string;
  label: string;
}

export interface AppConfig {
  name: string;
  version: string;
  sizeLimit: number;
  discussion: boolean;
  password: boolean;
  fileUpload: boolean;
  burnSelected: boolean;
  defaultFormatter: string;
  expireOptions: ExpireOption[];
  expireDefault: string;
}

const EXPIRE_LABELS: Record<string, string> = {
  '5min': '5 minutes',
  '10min': '10 minutes',
  '1hour': '1 hour',
  '1day': '1 day',
  '1week': '1 week',
  '1month': '1 month',
  '1year': '1 year',
  never: 'Never',
};

export function expireLabel(value: string): string {
  return EXPIRE_LABELS[value] ?? value;
}

const DEFAULTS: AppConfig = {
  name: 'PrivateBin',
  version: '',
  sizeLimit: 10 * 1024 * 1024,
  discussion: true,
  password: true,
  fileUpload: true,
  burnSelected: false,
  defaultFormatter: 'plaintext',
  expireOptions: Object.keys(EXPIRE_LABELS).map((value) => ({ value, label: EXPIRE_LABELS[value] })),
  expireDefault: '1week',
};

type Raw = Record<string, unknown>;

function pick<T>(raw: Raw, keys: string[], guard: (v: unknown) => v is T, fallback: T): T {
  for (const key of keys) {
    if (guard(raw[key])) return raw[key] as T;
  }
  return fallback;
}
const isBool = (v: unknown): v is boolean => typeof v === 'boolean';
const isNum = (v: unknown): v is number => typeof v === 'number';
const isStr = (v: unknown): v is string => typeof v === 'string';

function parseExpire(raw: Raw): Pick<AppConfig, 'expireOptions' | 'expireDefault'> {
  const expire = (raw.expire ?? {}) as Raw;
  const list = expire.options ?? raw.expire_options;
  let options = DEFAULTS.expireOptions;
  if (Array.isArray(list)) {
    options = list
      .map((item) => {
        if (typeof item === 'string') return item;
        if (item && typeof item === 'object') return String((item as Raw).key ?? (item as Raw).value ?? (item as Raw).name ?? '');
        return '';
      })
      .filter(Boolean)
      .map((value) => ({ value, label: expireLabel(value) }));
  } else if (list && typeof list === 'object') {
    options = Object.keys(list).map((value) => ({ value, label: expireLabel(value) }));
  }
  const def = pick(expire, ['default'], isStr, pick(raw, ['expire_default'], isStr, DEFAULTS.expireDefault));
  return { expireOptions: options.length ? options : DEFAULTS.expireOptions, expireDefault: def };
}

function load(): AppConfig {
  const el = document.getElementById('pb-config');
  let raw: Raw = {};
  try {
    const text = el?.textContent?.trim();
    if (text && !text.startsWith('<!--')) raw = JSON.parse(text) as Raw;
  } catch {
    // fall back to defaults
  }
  return {
    name: pick(raw, ['name'], isStr, DEFAULTS.name),
    version: pick(raw, ['version'], isStr, DEFAULTS.version),
    sizeLimit: pick(raw, ['size_limit', 'sizeLimit'], isNum, DEFAULTS.sizeLimit),
    discussion: pick(raw, ['discussion'], isBool, DEFAULTS.discussion),
    password: pick(raw, ['password'], isBool, DEFAULTS.password),
    fileUpload: pick(raw, ['file_upload', 'fileUpload'], isBool, DEFAULTS.fileUpload),
    burnSelected: pick(raw, ['burn_after_reading_selected', 'burnAfterReadingSelected'], isBool, DEFAULTS.burnSelected),
    defaultFormatter: pick(raw, ['default_formatter', 'defaultFormatter'], isStr, DEFAULTS.defaultFormatter),
    ...parseExpire(raw),
  };
}

export const config: AppConfig = load();
