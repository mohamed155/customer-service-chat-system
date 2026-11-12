import { Injectable, signal } from '@angular/core';
import { Catalog, EN, Entry, MessageKey, Params } from './messages';

/**
 * Looks up the widget's user-facing text (ADR-016). English is bundled; any other locale is a JSON
 * file next to the widget (`i18n/<locale>.json`) fetched at run time. A key a locale does not have
 * falls back to English, then to the key itself, so a half-finished translation never shows blanks.
 */
@Injectable({ providedIn: 'root' })
export class I18n {
  private readonly active = signal<Catalog>({});
  private readonly localeSignal = signal('en');
  private rules = new Intl.PluralRules('en');

  readonly locale = this.localeSignal.asReadonly();

  /** The message for `key`, with `{name}` placeholders filled in from `params`. */
  t(key: MessageKey, params: Params = {}): string {
    const entry: Entry = this.active()[key] ?? EN[key] ?? key;
    const text = typeof entry === 'string' ? entry : this.plural(entry, params['count']);
    return text.replace(/\{(\w+)\}/g, (whole, name: string) =>
      name in params ? String(params[name]) : whole,
    );
  }

  /**
   * Switch to `requested` (a BCP 47 tag such as `de` or `pt-BR`). English, an unknown locale and a
   * failed load all leave the English text in place.
   */
  async use(requested: string | null | undefined, base: string = location.href): Promise<void> {
    const locale = normalise(requested);
    // English is bundled, in every region.
    if (!locale || locale.split('-')[0] === 'en') {
      this.set('en', {});
      return;
    }
    for (const candidate of candidates(locale)) {
      try {
        const response = await fetch(new URL(`i18n/${candidate}.json`, base));
        if (response.ok) {
          this.set(candidate, parseCatalog(await response.json()));
          return;
        }
      } catch {
        // Offline or blocked: fall through to the next candidate, then to English.
      }
    }
    this.set('en', {});
  }

  private set(locale: string, catalog: Catalog): void {
    this.rules = safeRules(locale);
    this.active.set(catalog);
    this.localeSignal.set(locale);
  }

  private plural(forms: Exclude<Entry, string>, count: string | number | undefined): string {
    const n = Number(count);
    const category = Number.isFinite(n) ? this.rules.select(n) : 'other';
    return forms[category] ?? forms.other;
  }
}

/** `pt_br`, `PT-BR` and ` pt-BR ` are all `pt-BR`; anything that is not a locale tag is `null`. */
function normalise(requested: string | null | undefined): string | null {
  const tag = (requested ?? '').trim().replace('_', '-');
  if (!/^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*$/.test(tag)) return null;
  const [language, ...rest] = tag.split('-');
  return [language.toLowerCase(), ...rest.map((p) => (p.length === 2 ? p.toUpperCase() : p))].join(
    '-',
  );
}

/** `pt-BR` is tried as `pt-BR`, then `pt`. */
function candidates(locale: string): string[] {
  const language = locale.split('-')[0];
  return locale === language ? [locale] : [locale, language];
}

function safeRules(locale: string): Intl.PluralRules {
  try {
    return new Intl.PluralRules(locale);
  } catch {
    return new Intl.PluralRules('en');
  }
}

/** Keep only entries of the two allowed shapes: a translation file is data, not code. */
function parseCatalog(raw: unknown): Catalog {
  if (typeof raw !== 'object' || raw === null) return {};
  const out: Record<string, Entry> = {};
  for (const [key, value] of Object.entries(raw)) {
    if (typeof value === 'string') {
      out[key] = value;
    } else if (typeof value === 'object' && value !== null) {
      const forms = value as Record<string, unknown>;
      if (typeof forms['other'] === 'string') {
        const plural: Record<string, string> = { other: forms['other'] };
        for (const form of ['zero', 'one', 'two', 'few', 'many']) {
          if (typeof forms[form] === 'string') plural[form] = forms[form] as string;
        }
        out[key] = plural as unknown as Entry;
      }
    }
  }
  return out;
}
