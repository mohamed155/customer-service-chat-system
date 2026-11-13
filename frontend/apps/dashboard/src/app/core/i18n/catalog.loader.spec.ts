import { HttpBackend, HttpResponse, HttpRequest } from '@angular/common/http';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of } from 'rxjs';
import { CatalogLoader, toCatalog } from './catalog.loader';

describe('toCatalog', () => {
  it('keeps nested objects and strings only', () => {
    expect(toCatalog({ a: 'x', b: { c: 'y', d: 5, e: null, f: ['z'] }, g: 3 })).toEqual({
      a: 'x',
      b: { c: 'y' },
    });
  });

  it('drops keys that could reach a prototype', () => {
    const catalog = toCatalog(
      JSON.parse('{"__proto__": {"polluted": "yes"}, "constructor": "x", "ok": "y"}'),
    );
    expect(catalog).toEqual({ ok: 'y' });
    expect(({} as Record<string, unknown>)['polluted']).toBeUndefined();
  });

  it('refuses a value that is not an object', () => {
    expect(toCatalog('text')).toBeNull();
    expect(toCatalog(null)).toBeNull();
    expect(toCatalog([])).toBeNull();
  });
});

describe('CatalogLoader', () => {
  const requests: string[] = [];
  let body: unknown = { hello: 'Hallo' };
  let status = 200;

  function setup() {
    requests.length = 0;
    TestBed.configureTestingModule({
      providers: [
        {
          provide: HttpBackend,
          useValue: {
            handle: (req: HttpRequest<unknown>) => {
              requests.push(req.urlWithParams);
              return of(new HttpResponse({ status, body }));
            },
          },
        },
      ],
    });
    return TestBed.inject(CatalogLoader);
  }

  beforeEach(() => {
    body = { hello: 'Hallo' };
    status = 200;
  });

  it('serves the English shared catalog from the build, with no request', async () => {
    const loader = setup();
    const catalog = await firstValueFrom(loader.getTranslation('en'));
    expect(catalog).toHaveProperty('sidebar');
    expect(requests).toEqual([]);
  });

  it('fetches another locale as a file beside the app', async () => {
    const loader = setup();
    expect(await firstValueFrom(loader.getTranslation('de'))).toEqual({ hello: 'Hallo' });
    expect(requests).toEqual(['i18n/shared/de.json']);
  });

  it('reads a file once: a second ask is answered from memory', async () => {
    const loader = setup();
    await firstValueFrom(loader.getTranslation('de'));
    await firstValueFrom(loader.getTranslation('de'));
    expect(requests).toEqual(['i18n/shared/de.json']);
  });

  it('does not remember a file that failed', async () => {
    body = '<!doctype html>';
    const loader = setup();
    await expect(firstValueFrom(loader.getTranslation('fr'))).rejects.toThrow();
    body = { hello: 'Bonjour' };
    expect(await firstValueFrom(loader.getTranslation('fr'))).toEqual({ hello: 'Bonjour' });
    expect(requests).toEqual(['i18n/shared/fr.json', 'i18n/shared/fr.json']);
  });

  it('fetches a scope file for a locale, and English of an unregistered scope too', async () => {
    const loader = setup();
    await firstValueFrom(loader.getTranslation('knowledge/de'));
    await firstValueFrom(loader.getTranslation('knowledge/en'));
    expect(requests).toEqual(['i18n/knowledge/de.json', 'i18n/knowledge/en.json']);
  });

  it('fails when the file is not a catalog, as a page that is not JSON would', async () => {
    body = '<!doctype html>';
    const loader = setup();
    await expect(firstValueFrom(loader.getTranslation('fr'))).rejects.toThrow('not an object');
  });

  it('fails on a name that could leave the catalog folder', async () => {
    const loader = setup();
    await expect(firstValueFrom(loader.getTranslation('../secret'))).rejects.toThrow(
      'Bad catalog name',
    );
    await expect(firstValueFrom(loader.getTranslation('a/b/../c'))).rejects.toThrow(
      'Bad catalog name',
    );
    expect(requests).toEqual([]);
  });
});
