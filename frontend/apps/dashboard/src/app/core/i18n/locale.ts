/** The language every catalog starts from. English is always complete (ADR-016, section 4). */
export const DEFAULT_LOCALE = 'en';

/** Where a person's own language choice is kept in this browser (MOH-155). */
export const LOCALE_STORAGE_KEY = 'app.locale';

/** How many language tags are tried, so a browser that lists many does not cause many requests. */
const MAX_CANDIDATES = 6;

/**
 * A language tag in its canonical form (`de-AT`, `zh-Hant`), or `null` when the value is not a
 * well-formed tag. A tag becomes part of a catalog file name, so only a valid one gets through.
 */
export function parseLocale(value: unknown): string | null {
  if (typeof value !== 'string' || value.length === 0 || value.length > 35) return null;
  try {
    const [canonical] = Intl.getCanonicalLocales(value);
    return canonical && /^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*$/.test(canonical) ? canonical : null;
  } catch {
    return null;
  }
}

/** `de-AT` → `de`. */
export function baseLanguage(tag: string): string {
  return tag.split('-')[0].toLowerCase();
}

/**
 * The catalog files to look for, best first: the person's own choice, then the browser's languages
 * in order. Each tag is tried as written and then as its base language (`fr-CA`, then `fr`). Every
 * English tag is the one English catalog, which is bundled, so the list stops there: a later,
 * less preferred language is never reached.
 */
export function candidateLocales(
  stored: string | null,
  browser: readonly string[],
): readonly string[] {
  const out: string[] = [];
  const add = (tag: string) => {
    if (!out.includes(tag) && out.length < MAX_CANDIDATES) out.push(tag);
  };
  for (const raw of [stored, ...browser]) {
    const tag = parseLocale(raw);
    if (tag === null) continue;
    if (baseLanguage(tag) === DEFAULT_LOCALE) {
      add(DEFAULT_LOCALE);
      break;
    }
    add(tag);
    add(baseLanguage(tag));
  }
  if (!out.includes(DEFAULT_LOCALE)) out.push(DEFAULT_LOCALE);
  return out;
}

/**
 * The locale numbers and dates are written in: the first tag the person asked for whose language
 * is the one the text is shown in (`en-GB` when the text is English), otherwise the text's own
 * locale. Intl needs no data file per locale, so this is not limited to a list.
 */
export function formattingLocale(
  active: string,
  stored: string | null,
  browser: readonly string[],
): string {
  for (const raw of [stored, ...browser]) {
    const tag = parseLocale(raw);
    if (tag !== null && baseLanguage(tag) === baseLanguage(active)) return tag;
  }
  return active;
}

/** The scripts that are written from right to left. */
const RTL_SCRIPTS = new Set([
  'Arab',
  'Hebr',
  'Thaa',
  'Syrc',
  'Nkoo',
  'Adlm',
  'Rohg',
  'Mand',
  'Samr',
]);

/** Languages whose usual script is right to left, for a browser that cannot work the script out. */
const RTL_LANGUAGES = new Set([
  'ar',
  'he',
  'iw',
  'fa',
  'ur',
  'ps',
  'sd',
  'ug',
  'yi',
  'dv',
  'ckb',
  'ks',
]);

/**
 * The direction text runs in a language (MOH-73): `rtl` for Arabic, Hebrew, Persian, Urdu and the
 * other right-to-left languages, `ltr` for everything else. The script decides when the tag has
 * one (`ku-Arab` is right to left, `ku-Latn` is not), then the language's usual script.
 */
export function textDirection(tag: string): 'ltr' | 'rtl' {
  try {
    const script = new Intl.Locale(tag).maximize().script;
    if (script) return RTL_SCRIPTS.has(script) ? 'rtl' : 'ltr';
  } catch {
    // An old browser without Intl.Locale, or a tag it cannot read: go by the language.
  }
  // The script written in the tag (`ku-Arab`, `ar-Latn`) counts before the language's usual one.
  const written = tag
    .split('-')
    .slice(1)
    .find((part) => /^[A-Za-z]{4}$/.test(part));
  if (written) {
    return RTL_SCRIPTS.has(written[0].toUpperCase() + written.slice(1).toLowerCase())
      ? 'rtl'
      : 'ltr';
  }
  return RTL_LANGUAGES.has(baseLanguage(tag)) ? 'rtl' : 'ltr';
}
