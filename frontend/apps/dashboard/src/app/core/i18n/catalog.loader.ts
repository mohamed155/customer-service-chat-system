import { HttpBackend, HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import type { Translation, TranslocoLoader } from '@jsverse/transloco';
import { from, map, Observable, of, throwError } from 'rxjs';
import authEnglish from '../../../../public/i18n/auth/en.json';
import sharedEnglish from '../../../../public/i18n/shared/en.json';
import { DEFAULT_LOCALE } from './locale';

/**
 * The English catalog of each feature scope, loaded with the scope's lazy route. English is part
 * of the build, so a page never waits for a request to show its own text. Add a line when a
 * feature area gets a scope (frontend/CLAUDE.md, "User-facing text"). The `shared` scope is the
 * default catalog and is bundled with the app.
 */
export const ENGLISH_SCOPES: Readonly<Record<string, () => Promise<Translation>>> = {
  // Part of the initial bundle on purpose: the sign-in page is the first screen of a signed-out
  // visit, and its text must not wait for a second request.
  auth: () => Promise.resolve(authEnglish),
  platform: () => import('../../../../public/i18n/platform/en.json').then((m) => m.default),
  integrations: () => import('../../../../public/i18n/integrations/en.json').then((m) => m.default),
  settings: () => import('../../../../public/i18n/settings/en.json').then((m) => m.default),
  'knowledge-base': () =>
    import('../../../../public/i18n/knowledge-base/en.json').then((m) => m.default),
  'ai-agent': () => import('../../../../public/i18n/ai-agent/en.json').then((m) => m.default),
  team: () => import('../../../../public/i18n/team/en.json').then((m) => m.default),
  customers: () => import('../../../../public/i18n/customers/en.json').then((m) => m.default),
  conversations: () =>
    import('../../../../public/i18n/conversations/en.json').then((m) => m.default),
};

/** A catalog is data: only nested objects and strings are kept, and no key can reach a prototype. */
export function toCatalog(value: unknown): Translation | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const out: Record<string, unknown> = Object.create(null);
  for (const [key, entry] of Object.entries(value)) {
    if (key === '__proto__' || key === 'constructor' || key === 'prototype') continue;
    if (typeof entry === 'string') {
      out[key] = entry;
    } else {
      const nested = toCatalog(entry);
      if (nested !== null) out[key] = nested;
    }
  }
  return out as Translation;
}

/**
 * Transloco asks for `<locale>` (the shared catalog) or `<scope>/<locale>`. English comes from the
 * build; any other locale is the file `i18n/<scope>/<locale>.json` next to the app, so a new
 * language is a new file and no code. A missing file is a failed load (a 404, or a page that is not
 * JSON), which is how the locale service knows a language is not offered.
 */
@Injectable({ providedIn: 'root' })
export class CatalogLoader implements TranslocoLoader {
  // Straight to the network: the API interceptors (token, tenant, request ID, retries) are for
  // API calls, and a catalog is a static file.
  private readonly http = new HttpClient(inject(HttpBackend));
  // Files already read: the locale service reads a language once to learn it exists, and
  // Transloco asks for it again a moment later.
  private readonly fetched = new Map<string, Translation>();

  getTranslation(path: string): Observable<Translation> {
    const slash = path.lastIndexOf('/');
    const scope = slash === -1 ? 'shared' : path.slice(0, slash);
    const locale = slash === -1 ? path : path.slice(slash + 1);
    if (!/^[A-Za-z0-9_-]+(?:\/[A-Za-z0-9_-]+)*$/.test(`${scope}/${locale}`)) {
      return throwError(() => new Error(`Bad catalog name "${path}"`));
    }
    if (locale === DEFAULT_LOCALE) {
      if (scope === 'shared') return of(toCatalog(sharedEnglish) ?? {});
      const english = ENGLISH_SCOPES[scope];
      if (english) return from(english()).pipe(map((catalog) => toCatalog(catalog) ?? {}));
    }
    const file = `i18n/${scope}/${locale}.json`;
    const known = this.fetched.get(file);
    if (known) return of(known);
    return this.http.get<unknown>(file).pipe(
      map((body) => {
        const catalog = toCatalog(body);
        if (catalog === null) throw new Error(`Catalog "${scope}/${locale}" is not an object`);
        this.fetched.set(file, catalog);
        return catalog;
      }),
    );
  }
}
