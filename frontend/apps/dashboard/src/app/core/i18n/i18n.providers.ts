import { EnvironmentProviders, Provider, inject, provideAppInitializer } from '@angular/core';
import {
  TRANSLOCO_LOADER,
  TRANSLOCO_TRANSPILER,
  TranslocoConfig,
  provideTransloco,
  translocoConfig,
} from '@jsverse/transloco';
import { TitleStrategy } from '@angular/router';
import { TUI_LANGUAGE } from '@taiga-ui/i18n/tokens';
import { CatalogLoader } from './catalog.loader';
import { IcuTranspiler } from './icu-transpiler';
import { DEFAULT_LOCALE } from './locale';
import { LocaleService } from './locale.service';
import { TranslatedTitleStrategy } from './title.strategy';

/**
 * Transloco's settings. English is the default and the fallback: a key missing in the active
 * language is looked up in English, then the key itself is shown (never an empty label).
 */
export function i18nConfig(overrides: Partial<TranslocoConfig> = {}): TranslocoConfig {
  return translocoConfig({
    defaultLang: DEFAULT_LOCALE,
    // More are added as languages are found (LocaleService); Transloco needs the list to tell a
    // language from a scope.
    availableLangs: [DEFAULT_LOCALE],
    // Catalogs are files next to the app: a failed load will not succeed on a retry.
    failedRetries: 0,
    fallbackLang: DEFAULT_LOCALE,
    reRenderOnLangChange: true,
    missingHandler: { useFallbackTranslation: true },
    // A scope is written as its name (`ai-agent.page.title`), not in camel case (`aiAgent`).
    scopes: { autoPrefixKeys: true, keepCasing: true },
    ...overrides,
  });
}

/**
 * Text, dates and numbers in the active language (MOH-155, ADR-016):
 * Transloco with the catalogs under `public/i18n/<scope>/<locale>.json`, ICU messages, the
 * language chosen at start-up, and Taiga UI's own text following it.
 */
export function provideI18n(): (Provider | EnvironmentProviders)[] {
  return [
    provideTransloco({ config: i18nConfig(), loader: CatalogLoader }),
    // One loader for Transloco and the locale service, so a file is read once.
    { provide: TRANSLOCO_LOADER, useExisting: CatalogLoader },
    { provide: TRANSLOCO_TRANSPILER, useClass: IcuTranspiler },
    { provide: TitleStrategy, useExisting: TranslatedTitleStrategy },
    { provide: TUI_LANGUAGE, useFactory: () => inject(LocaleService).taigaLanguage },
    provideAppInitializer(() => inject(LocaleService).init()),
  ];
}
