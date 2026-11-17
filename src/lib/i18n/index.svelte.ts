import { de } from "./de";
import { en, type MessageKey } from "./en";

export type Locale = "en" | "de";
export type { MessageKey };

/** Backend message: stable code + params, English fallback text. */
export interface Msg {
  code: string;
  params?: Record<string, string>;
  text: string;
}

const STORAGE_KEY = "terminalox.locale";
const dictionaries: Record<Locale, Record<MessageKey, string>> = { en, de };

function detect(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "en" || saved === "de") return saved;
  } catch {
    // Storage unavailable: fall back to the system language.
  }
  const lang = typeof navigator !== "undefined" ? navigator.language : "en";
  return lang.toLowerCase().startsWith("de") ? "de" : "en";
}

class I18n {
  locale = $state<Locale>(detect());

  /** BCP 47 tag for Intl formatting. */
  get tag(): string {
    return this.locale === "de" ? "de-DE" : "en-US";
  }

  setLocale(l: Locale) {
    this.locale = l;
    try {
      localStorage.setItem(STORAGE_KEY, l);
    } catch {
      // Only a convenience; the system language is used next time.
    }
    document.documentElement.lang = l;
  }
}

export const i18n = new I18n();

function interpolate(s: string, params?: Record<string, string | number>): string {
  if (!params) return s;
  return s.replace(/\{(\w+)\}/g, (m, k) => (k in params ? String(params[k]) : m));
}

/** Translate a UI string. `{name}` placeholders are filled from `params`. */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  const dict = dictionaries[i18n.locale];
  return interpolate(dict[key] ?? en[key] ?? key, params);
}

/** Singular/plural: uses `<key>_one` when count is 1, else `<key>_other`. */
export function tn(
  key: string,
  count: number,
  params?: Record<string, string | number>,
): string {
  const k = (count === 1 ? `${key}_one` : `${key}_other`) as MessageKey;
  return t(k, { count, ...params });
}

function isMsg(v: unknown): v is Msg {
  return !!v && typeof v === "object" && "code" in v && "text" in v;
}

/** Translate a backend message; unknown codes fall back to its English text. */
export function tm(m: Msg | string | null | undefined): string {
  if (m == null) return "";
  if (typeof m === "string") return m;
  const key = `msg.${m.code}` as MessageKey;
  const dict = dictionaries[i18n.locale];
  const tpl = dict[key] ?? en[key];
  return tpl ? interpolate(tpl, m.params) : m.text;
}

/** Any error (backend object, string, Error) as translated text. */
export function errorText(e: unknown): string {
  if (isMsg(e)) return tm(e);
  if (e instanceof Error) return e.message;
  return String(e);
}

export function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString(i18n.tag);
}

/** "5 minutes ago" / "vor 5 Minuten". */
export function relativeTime(iso: string): string {
  const rtf = new Intl.RelativeTimeFormat(i18n.tag, { numeric: "auto" });
  const min = Math.round((Date.parse(iso) - Date.now()) / 60000);
  if (Math.abs(min) < 60) return rtf.format(min, "minute");
  const h = Math.round(min / 60);
  if (Math.abs(h) < 24) return rtf.format(h, "hour");
  return rtf.format(Math.round(h / 24), "day");
}
