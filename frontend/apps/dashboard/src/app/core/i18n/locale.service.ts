import { DOCUMENT } from '@angular/common';
import { DestroyRef, Injectable, inject, signal } from '@angular/core';
import { TranslocoService } from '@jsverse/transloco';
import { TUI_ENGLISH_LANGUAGE } from '@taiga-ui/i18n/languages/english';
import type { TuiLanguage } from '@taiga-ui/i18n/types';
import { filter, firstValueFrom } from 'rxjs';
import { CatalogLoader } from './catalog.loader';
import {
  DEFAULT_LOCALE,
  LOCALE_STORAGE_KEY,
  candidateLocales,
  formattingLocale,
  baseLanguage,
  parseLocale,
} from './locale';

/**
 * The language the dashboard shows (MOH-155, ADR-016).
 *
 * The language is the person's own choice (kept in this browser; the API has no per-user language
 * setting yet), else the first browser language that has a catalog file, else English. A language
 * "has a catalog" when `i18n/shared/<locale>.json` loads, so adding a language is adding files.
 * Nothing in the app lets a person pick one yet; `setLocale()` is what such a screen will call.
 *
 * `locale` is the language of the text. `formatLocale` is what dates and numbers are written in
 * (`en-GB` for an English page in a British browser). Taiga's own text follows `taigaLanguage`.
 */
@Injectable({ providedIn: 'root' })
export class LocaleService {
  private readonly transloco = inject(TranslocoService);
  private readonly catalogs = inject(CatalogLoader);
  private readonly document = inject(DOCUMENT);

  private readonly active = signal(DEFAULT_LOCALE);
  /** Counts catalogs that finished loading: a scope's file arrives after the first render. */
  private readonly loadedCatalogs = signal(0);
  private readonly formatting = signal(DEFAULT_LOCALE);
  private readonly taiga = signal<TuiLanguage>(TUI_ENGLISH_LANGUAGE);

  readonly locale = this.active.asReadonly();
  readonly formatLocale = this.formatting.asReadonly();
  readonly taigaLanguage = this.taiga.asReadonly();

  constructor() {
    const loads = this.transloco.events$
      .pipe(filter((event) => event.type === 'translationLoadSuccess'))
      // The event can fire while a template is being rendered (a pipe asks for its scope's
      // catalog), where writing a signal is an error: do it right after.
      .subscribe(() => queueMicrotask(() => this.loadedCatalogs.update((n) => n + 1)));
    inject(DestroyRef).onDestroy(() => loads.unsubscribe());
  }

  /**
   * Text for a key in the active language, for code that cannot use the `transloco` pipe. A
   * `computed()` or `effect()` that calls it runs again when the language changes and when a
   * catalog finishes loading (an area's file is fetched with its lazy route, after the first
   * render, so a key read before that would otherwise stay as the key for good).
   */
  translate(key: string, params?: Record<string, unknown>): string {
    this.active();
    this.loadedCatalogs();
    return this.transloco.translate(key, params);
  }

  /** Picks the language at start-up: the stored choice, then the browser's, then English. */
  async init(): Promise<void> {
    const stored = this.storedChoice();
    for (const candidate of candidateLocales(stored, this.browserLanguages())) {
      if (await this.activate(candidate, stored)) return;
    }
  }

  /**
   * Switches to a language and remembers it. Returns `false`, and changes nothing, when the tag is
   * not valid or the language has no catalog.
   */
  async setLocale(tag: string): Promise<boolean> {
    const locale = parseLocale(tag);
    if (locale === null || !(await this.activate(locale, locale))) return false;
    this.write(locale);
    return true;
  }

  /** Forgets the stored choice and goes back to the browser's language. */
  async clearChoice(): Promise<void> {
    this.write(null);
    await this.init();
  }

  private async activate(locale: string, requested: string | null): Promise<boolean> {
    try {
      // English is the fallback of every language, so it is always loaded (it is part of the build).
      await firstValueFrom(this.transloco.load(DEFAULT_LOCALE));
      if (locale !== DEFAULT_LOCALE) {
        // Read the file first: Transloco must know a language before it loads one (a name it does
        // not know is taken for a scope), and a language without a file must not be added.
        const catalog = await firstValueFrom(this.catalogs.getTranslation(locale));
        const known = this.transloco.getAvailableLangs() as string[];
        if (!known.includes(locale)) this.transloco.setAvailableLangs([...known, locale]);
        this.transloco.setTranslation(catalog, locale);
      }
    } catch {
      return false;
    }
    this.transloco.setActiveLang(locale);
    this.active.set(locale);
    this.formatting.set(formattingLocale(locale, requested, this.browserLanguages()));
    this.document.documentElement.lang = locale;
    await this.loadTaiga(locale);
    return true;
  }

  private async loadTaiga(locale: string): Promise<void> {
    if (baseLanguage(locale) === DEFAULT_LOCALE) {
      this.taiga.set(TUI_ENGLISH_LANGUAGE);
      return;
    }
    try {
      // The table of Taiga's languages is its own chunk, so English users never fetch it.
      const { loadTaigaLanguage } = await import('./taiga-language');
      const loading = loadTaigaLanguage(locale);
      this.taiga.set(loading ? await loading : TUI_ENGLISH_LANGUAGE);
    } catch {
      // Taiga's own text stays English; the app's text has already switched.
      this.taiga.set(TUI_ENGLISH_LANGUAGE);
    }
  }

  private browserLanguages(): readonly string[] {
    const nav = this.document.defaultView?.navigator;
    return nav?.languages?.length ? nav.languages : nav?.language ? [nav.language] : [];
  }

  private storedChoice(): string | null {
    try {
      return parseLocale(this.document.defaultView?.localStorage.getItem(LOCALE_STORAGE_KEY));
    } catch {
      return null;
    }
  }

  private write(locale: string | null): void {
    try {
      const storage = this.document.defaultView?.localStorage;
      if (locale === null) storage?.removeItem(LOCALE_STORAGE_KEY);
      else storage?.setItem(LOCALE_STORAGE_KEY, locale);
    } catch {
      // Storage is blocked: the choice lasts until the page closes.
    }
  }
}
