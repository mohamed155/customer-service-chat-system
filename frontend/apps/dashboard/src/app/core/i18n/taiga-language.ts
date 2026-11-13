import type { TuiLanguage } from '@taiga-ui/i18n/types';
import { baseLanguage } from './locale';

/**
 * Taiga UI's own text (calendar months, validation messages, "Close") for the languages it ships.
 * Each one is a separate lazy chunk, fetched only when that language is active; English is the
 * default Taiga already holds. A language Taiga has no file for keeps English for Taiga's text
 * while the app's own text follows the catalog.
 */
const TAIGA_LANGUAGES: Readonly<Record<string, () => Promise<TuiLanguage>>> = {
  ar: () => import('@taiga-ui/i18n/languages/arabic').then((m) => m.TUI_ARABIC_LANGUAGE),
  be: () => import('@taiga-ui/i18n/languages/belarusian').then((m) => m.TUI_BELARUSIAN_LANGUAGE),
  de: () => import('@taiga-ui/i18n/languages/german').then((m) => m.TUI_GERMAN_LANGUAGE),
  el: () => import('@taiga-ui/i18n/languages/greek').then((m) => m.TUI_GREEK_LANGUAGE),
  es: () => import('@taiga-ui/i18n/languages/spanish').then((m) => m.TUI_SPANISH_LANGUAGE),
  fr: () => import('@taiga-ui/i18n/languages/french').then((m) => m.TUI_FRENCH_LANGUAGE),
  he: () => import('@taiga-ui/i18n/languages/hebrew').then((m) => m.TUI_HEBREW_LANGUAGE),
  it: () => import('@taiga-ui/i18n/languages/italian').then((m) => m.TUI_ITALIAN_LANGUAGE),
  ja: () => import('@taiga-ui/i18n/languages/japanese').then((m) => m.TUI_JAPANESE_LANGUAGE),
  kk: () => import('@taiga-ui/i18n/languages/kazakh').then((m) => m.TUI_KAZAKH_LANGUAGE),
  ko: () => import('@taiga-ui/i18n/languages/korean').then((m) => m.TUI_KOREAN_LANGUAGE),
  lt: () => import('@taiga-ui/i18n/languages/lithuanian').then((m) => m.TUI_LITHUANIAN_LANGUAGE),
  ms: () => import('@taiga-ui/i18n/languages/malay').then((m) => m.TUI_MALAY_LANGUAGE),
  nl: () => import('@taiga-ui/i18n/languages/dutch').then((m) => m.TUI_DUTCH_LANGUAGE),
  pl: () => import('@taiga-ui/i18n/languages/polish').then((m) => m.TUI_POLISH_LANGUAGE),
  pt: () => import('@taiga-ui/i18n/languages/portuguese').then((m) => m.TUI_PORTUGUESE_LANGUAGE),
  ru: () => import('@taiga-ui/i18n/languages/russian').then((m) => m.TUI_RUSSIAN_LANGUAGE),
  tg: () => import('@taiga-ui/i18n/languages/tajik').then((m) => m.TUI_TAJIK_LANGUAGE),
  tr: () => import('@taiga-ui/i18n/languages/turkish').then((m) => m.TUI_TURKISH_LANGUAGE),
  uk: () => import('@taiga-ui/i18n/languages/ukrainian').then((m) => m.TUI_UKRAINIAN_LANGUAGE),
  vi: () => import('@taiga-ui/i18n/languages/vietnamese').then((m) => m.TUI_VIETNAMESE_LANGUAGE),
  zh: () => import('@taiga-ui/i18n/languages/chinese').then((m) => m.TUI_CHINESE_LANGUAGE),
};

/** Taiga's text for a locale, or `null` when Taiga has none (the caller keeps English). */
export function loadTaigaLanguage(locale: string): Promise<TuiLanguage> | null {
  return TAIGA_LANGUAGES[baseLanguage(locale)]?.() ?? null;
}

/** The base languages Taiga has text for (a test checks this list against the files). */
export const TAIGA_LANGUAGE_CODES: readonly string[] = Object.keys(TAIGA_LANGUAGES);
