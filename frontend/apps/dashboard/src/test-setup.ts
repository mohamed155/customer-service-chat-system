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
