import { TestBed } from '@angular/core/testing';
import { provideI18nTesting, takeMissingKeys } from './app/core/i18n/i18n.testing';

if (typeof globalThis.localStorage?.getItem !== 'function') {
  const values = new Map<string, string>();
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: {
      get length(): number {
        return values.size;
      },
      clear: () => values.clear(),
      getItem: (key: string) => values.get(key) ?? null,
      key: (index: number) => [...values.keys()][index] ?? null,
      removeItem: (key: string) => values.delete(key),
      setItem: (key: string, value: string) => values.set(key, String(value)),
    } satisfies Storage,
  });
}

if (typeof window.matchMedia !== 'function') {
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: (query: string): MediaQueryList => {
      const target = new EventTarget();
      return Object.assign(target, {
        matches: false,
        media: query,
        onchange: null,
        addListener: (listener: (event: MediaQueryListEvent) => void) =>
          target.addEventListener('change', listener as EventListener),
        removeListener: (listener: (event: MediaQueryListEvent) => void) =>
          target.removeEventListener('change', listener as EventListener),
        dispatchEvent: target.dispatchEvent.bind(target),
      }) as MediaQueryList;
    },
  });
}

// Every test renders text from the real English catalog, and a key the catalog lacks fails the test
// that renders it (MOH-155). Providers a spec adds are merged on top of these.
beforeEach(() => {
  takeMissingKeys();
  TestBed.configureTestingModule({ providers: [provideI18nTesting()] });
});

// A template's pipe reports a missing key from inside a subscription, so the error may not reach
// the test; the handler also notes the key, and the test fails here.
afterEach(() => {
  const missing = takeMissingKeys();
  if (missing.length) {
    throw new Error(
      `Keys missing from the English catalog (apps/dashboard/public/i18n): ${missing.join(', ')}`,
    );
  }
});
