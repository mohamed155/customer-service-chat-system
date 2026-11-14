import {
  EnvironmentProviders,
  Provider,
  inject,
  provideEnvironmentInitializer,
} from '@angular/core';
import {
  TRANSLOCO_MISSING_HANDLER,
  TRANSLOCO_TRANSPILER,
  TranslocoMissingHandler,
  TranslocoService,
  provideTransloco,
} from '@jsverse/transloco';
import type { Translation, TranslocoLoader } from '@jsverse/transloco';
import { Observable, of, throwError } from 'rxjs';
import authEnglish from '../../../../public/i18n/auth/en.json';
import conversationsEnglish from '../../../../public/i18n/conversations/en.json';
import customersEnglish from '../../../../public/i18n/customers/en.json';
import teamEnglish from '../../../../public/i18n/team/en.json';
import aiAgentEnglish from '../../../../public/i18n/ai-agent/en.json';
import sharedEnglish from '../../../../public/i18n/shared/en.json';
import { toCatalog } from './catalog.loader';
import { IcuTranspiler } from './icu-transpiler';
import { i18nConfig } from './i18n.providers';

/**
 * The English catalog of each scope for component tests, loaded without a request so a template
 * shows its text on the first change detection. Add a line next to `ENGLISH_SCOPES` in
 * `catalog.loader.ts` when a feature area gets a scope.
 */
const TEST_SCOPES: Readonly<Record<string, Translation>> = {
  auth: authEnglish,
  conversations: conversationsEnglish,
  customers: customersEnglish,
  team: teamEnglish,
  'ai-agent': aiAgentEnglish,
};

class EnglishTestLoader implements TranslocoLoader {
  getTranslation(path: string): Observable<Translation> {
    if (path === 'en') return of(toCatalog(sharedEnglish) ?? {});
    const scope = path.endsWith('/en') ? TEST_SCOPES[path.slice(0, -3)] : undefined;
    return scope ? of(scope) : throwError(() => new Error(`No test catalog "${path}"`));
  }
}

const missingKeys: string[] = [];

/**
 * The keys asked for in this test that the English catalog lacks, which empties the list. The
 * pipe reports a failure from inside a subscription, where the error cannot reach the test, so
 * `test-setup.ts` checks the list after each test.
 */
export function takeMissingKeys(): string[] {
  return missingKeys.splice(0);
}

/** A key a template or a component uses that the English catalog lacks fails its test. */
export class StrictMissingHandler implements TranslocoMissingHandler {
  handle(key: string): never {
    missingKeys.push(key);
    throw new Error(`Missing translation key "${key}": add it to the English catalog.`);
  }
}

/**
 * Providers for a component test: the real English catalog and message format, and a missing key
 * throws. `test-setup.ts` adds them to every test; a spec needs nothing of its own.
 */
export function provideI18nTesting(): (Provider | EnvironmentProviders)[] {
  return [
    provideTransloco({ config: i18nConfig({ prodMode: false }), loader: EnglishTestLoader }),
    { provide: TRANSLOCO_TRANSPILER, useClass: IcuTranspiler },
    { provide: TRANSLOCO_MISSING_HANDLER, useClass: StrictMissingHandler },
    // Loaded before the test runs, so code that asks for text directly (not through the pipe)
    // finds it, as it does in the app after start-up.
    provideEnvironmentInitializer(() => {
      inject(TranslocoService).load('en').subscribe();
    }),
  ];
}
