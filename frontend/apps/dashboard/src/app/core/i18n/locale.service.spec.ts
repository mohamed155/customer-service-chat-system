import { DOCUMENT } from '@angular/common';
import { TestBed } from '@angular/core/testing';
import { TRANSLOCO_TRANSPILER, TranslocoService, provideTransloco } from '@jsverse/transloco';
import { TUI_ENGLISH_LANGUAGE } from '@taiga-ui/i18n/languages/english';
import { Observable, firstValueFrom, of, throwError } from 'rxjs';
import { CatalogLoader } from './catalog.loader';
import { i18nConfig } from './i18n.providers';
import { IcuTranspiler } from './icu-transpiler';
import { LOCALE_STORAGE_KEY } from './locale';
import { LocaleService } from './locale.service';

const CATALOGS: Record<string, Record<string, unknown>> = {
  en: { hello: 'Hello', onlyEnglish: 'Only in English' },
  de: { hello: 'Hallo' },
  'fr-CA': { hello: 'Allô' },
  fr: { hello: 'Bonjour' },
  tlh: { hello: 'nuqneH' },
};

class StubCatalogs {
  readonly asked: string[] = [];
  getTranslation(path: string): Observable<Record<string, unknown>> {
    this.asked.push(path);
    const catalog = CATALOGS[path];
    return catalog ? of(catalog) : throwError(() => new Error(`404 ${path}`));
  }
}

describe('LocaleService', () => {
  let browser: string[];
  let catalogs: StubCatalogs;

  function setup(stored?: string) {
    localStorage.removeItem(LOCALE_STORAGE_KEY);
    if (stored) localStorage.setItem(LOCALE_STORAGE_KEY, stored);
    catalogs = new StubCatalogs();
    vi.spyOn(window.navigator, 'languages', 'get').mockReturnValue(browser);
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [
        provideTransloco({
          config: i18nConfig({ prodMode: false }),
          loader: StubCatalogs as never,
        }),
        { provide: TRANSLOCO_TRANSPILER, useClass: IcuTranspiler },
        { provide: CatalogLoader, useValue: catalogs },
      ],
    });
    // Transloco's loader is the same stub, so a catalog is read the same way everywhere.
    TestBed.overrideProvider(StubCatalogs, { useValue: catalogs });
    return {
      locale: TestBed.inject(LocaleService),
      transloco: TestBed.inject(TranslocoService),
      doc: TestBed.inject(DOCUMENT),
    };
  }

  beforeEach(() => {
    browser = ['en-US'];
  });

  afterEach(() => {
    localStorage.removeItem(LOCALE_STORAGE_KEY);
    vi.restoreAllMocks();
  });

  it('starts in English for an English browser, with no request for a catalog', async () => {
    const { locale, doc } = setup();
    await locale.init();
    expect(locale.locale()).toBe('en');
    expect(locale.formatLocale()).toBe('en-US');
    expect(doc.documentElement.lang).toBe('en');
    expect(locale.taigaLanguage()).toBe(TUI_ENGLISH_LANGUAGE);
    expect(catalogs.asked).toEqual([]);
  });

  it("uses the browser's first language that has a catalog", async () => {
    browser = ['es-ES', 'de-AT', 'en'];
    const { locale, transloco, doc } = setup();
    await locale.init();
    expect(catalogs.asked).toEqual(['es-ES', 'es', 'de-AT', 'de']);
    expect(locale.locale()).toBe('de');
    expect(locale.formatLocale()).toBe('de-AT');
    expect(transloco.translate('hello')).toBe('Hallo');
    expect(doc.documentElement.lang).toBe('de');
  });

  it('prefers a more specific catalog over its base language', async () => {
    browser = ['fr-CA'];
    const { locale, transloco } = setup();
    await locale.init();
    expect(locale.locale()).toBe('fr-CA');
    expect(transloco.translate('hello')).toBe('Allô');
  });

  it('falls back to English for a key a catalog lacks, then to the key', async () => {
    browser = ['de'];
    const { locale, transloco } = setup();
    await locale.init();
    expect(transloco.translate('onlyEnglish')).toBe('Only in English');
    expect(transloco.translate('nobody.wrote.this')).toBe('nobody.wrote.this');
  });

  it('shows English when no language of the browser has a catalog', async () => {
    browser = ['ja', 'ko'];
    const { locale } = setup();
    await locale.init();
    expect(locale.locale()).toBe('en');
  });

  it("a stored choice beats the browser's language", async () => {
    browser = ['de'];
    const { locale } = setup('fr');
    await locale.init();
    expect(locale.locale()).toBe('fr');
  });

  it('ignores a stored value that is not a language tag', async () => {
    browser = ['de'];
    const { locale } = setup('../../x');
    await locale.init();
    expect(locale.locale()).toBe('de');
  });

  it('setLocale switches, remembers the choice, and a language without a file changes nothing', async () => {
    const { locale, transloco } = setup();
    await locale.init();
    expect(await locale.setLocale('klingon!')).toBe(false);
    expect(await locale.setLocale('xx')).toBe(false);
    expect(locale.locale()).toBe('en');
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBeNull();

    expect(await locale.setLocale('tlh')).toBe(true);
    expect(locale.locale()).toBe('tlh');
    expect(transloco.translate('hello')).toBe('nuqneH');
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe('tlh');
  });

  it('translate() follows a language change in a computed value', async () => {
    const { locale } = setup();
    await locale.init();
    const { computed } = await import('@angular/core');
    const text = TestBed.runInInjectionContext(() => computed(() => locale.translate('hello')));
    expect(text()).toBe('Hello');
    await locale.setLocale('de');
    expect(text()).toBe('Hallo');
  });

  it('translate() follows a catalog that loads after the first read', async () => {
    const { locale, transloco } = setup();
    await locale.init();
    const { computed } = await import('@angular/core');
    const text = TestBed.runInInjectionContext(() => computed(() => locale.translate('area.late')));
    expect(text()).toBe('area.late');
    CATALOGS['area/en'] = { late: 'Arrived' };
    try {
      await firstValueFrom(transloco.load('area/en'));
    } finally {
      delete CATALOGS['area/en'];
    }
    expect(text()).toBe('Arrived');
  });

  it('clearChoice forgets the choice and goes back to the browser language', async () => {
    browser = ['de'];
    const { locale } = setup('fr');
    await locale.init();
    expect(locale.locale()).toBe('fr');
    await locale.clearChoice();
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBeNull();
    expect(locale.locale()).toBe('de');
  });

  it('loads Taiga’s own text for the language, from its own chunk', async () => {
    browser = ['de'];
    const { locale } = setup();
    await locale.init();
    const { TUI_GERMAN_LANGUAGE } = await import('@taiga-ui/i18n/languages/german');
    expect(locale.taigaLanguage()).toBe(TUI_GERMAN_LANGUAGE);
    expect(locale.taigaLanguage()).not.toBe(TUI_ENGLISH_LANGUAGE);
  });

  it('keeps Taiga in English for a language it has no text for', async () => {
    const { locale } = setup();
    await locale.init();
    await locale.setLocale('tlh');
    expect(locale.taigaLanguage()).toBe(TUI_ENGLISH_LANGUAGE);
  });
});
