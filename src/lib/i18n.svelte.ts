// UI language. `t()` reads the reactive locale, so every template expression that calls it
// re-renders when the language changes. German is the source dictionary; the English one must
// define the same keys (checked by the type system).
import type { Language } from "./api";
import { de, type Key } from "./locales/de";
import { en } from "./locales/en";

export type Locale = "de" | "en";
export type { Key };

class I18n {
  locale = $state<Locale>("en");
  /** Language the operating system prefers; used when the setting is "system". */
  systemLocale: Locale = typeof navigator !== "undefined" && navigator.language.toLowerCase().startsWith("de") ? "de" : "en";

  setLanguage(language: Language) {
    const next: Locale = language === "system" ? this.systemLocale : language;
    if (next !== this.locale) this.locale = next;
    if (typeof document !== "undefined") document.documentElement.lang = next;
  }

  /** BCP 47 tag for Intl formatting: German dates/times, otherwise the browser's English variant. */
  get intlTag(): string {
    if (this.locale === "de") return "de-DE";
    const nav = typeof navigator !== "undefined" ? navigator.language : "";
    return nav.toLowerCase().startsWith("en") ? nav : "en-GB";
  }
}

export const i18n = new I18n();

const dicts: Record<Locale, Record<Key, string>> = { de, en };

export function t(key: Key, params?: Record<string, string | number>): string {
  let text: string = dicts[i18n.locale][key] ?? de[key] ?? key;
  if (params) for (const [k, v] of Object.entries(params)) text = text.replaceAll(`{${k}}`, String(v));
  return text;
}

/** Short weekday name, 0 = Monday … 6 = Sunday. */
export function weekdayShort(index: number): string {
  return t(`weekday.${index}` as Key);
}
