import { TestBed } from '@angular/core/testing';
import { Route } from '@angular/router';
import { Observable, of, throwError } from 'rxjs';
import { FALLBACK_IDLE_MS, IdlePreloadStrategy, PRELOAD_DELAY_MS } from './idle-preload.strategy';

describe('IdlePreloadStrategy', () => {
  const marked: Route = { path: 'inbox', data: { preload: true } };

  function strategy(): IdlePreloadStrategy {
    return TestBed.inject(IdlePreloadStrategy);
  }

  /** Subscribe to a preload and report what it did. */
  function run(route: Route, load: () => Observable<unknown>) {
    const seen = { loaded: 0, completed: false, error: undefined as unknown };
    const subscription = strategy()
      .preload(route, () => {
        seen.loaded++;
        return load();
      })
      .subscribe({
        complete: () => (seen.completed = true),
        error: (e) => (seen.error = e),
      });
    return { seen, subscription };
  }

  function setConnection(connection: unknown) {
    Object.defineProperty(window.navigator, 'connection', {
      value: connection,
      configurable: true,
    });
  }

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    delete (window.navigator as { connection?: unknown }).connection;
    delete (document as { readyState?: unknown }).readyState;
    delete (window as { requestIdleCallback?: unknown }).requestIdleCallback;
    delete (window as { cancelIdleCallback?: unknown }).cancelIdleCallback;
  });

  it('does not load a route that is not marked', () => {
    vi.useFakeTimers();
    const { seen } = run({ path: 'other', data: { requiredPermission: 'x' } }, () => of(null));
    vi.advanceTimersByTime(60_000);

    expect(seen.loaded).toBe(0);
    expect(seen.completed).toBe(true);
  });

  it('does not load a route with no data at all', () => {
    vi.useFakeTimers();
    const { seen } = run({ path: 'plain' }, () => of(null));
    vi.advanceTimersByTime(60_000);

    expect(seen.loaded).toBe(0);
  });

  it('loads a marked route when the browser is idle, not before', () => {
    vi.useFakeTimers();
    let idle: () => void = () => undefined;
    const requested = vi.fn();
    (window as unknown as { requestIdleCallback: unknown }).requestIdleCallback = (
      callback: () => void,
    ) => {
      requested();
      idle = callback;
      return 1;
    };
    (window as unknown as { cancelIdleCallback: unknown }).cancelIdleCallback = vi.fn();

    const { seen } = run(marked, () => of('chunk'));
    vi.advanceTimersByTime(PRELOAD_DELAY_MS - 1);
    expect(requested).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(requested).toHaveBeenCalledTimes(1);
    expect(seen.loaded).toBe(0);

    idle();

    expect(seen.loaded).toBe(1);
    expect(seen.completed).toBe(true);
  });

  it('waits a fixed time when the browser has no idle callback', () => {
    vi.useFakeTimers();
    const { seen } = run(marked, () => of('chunk'));

    vi.advanceTimersByTime(PRELOAD_DELAY_MS + FALLBACK_IDLE_MS - 1);
    expect(seen.loaded).toBe(0);
    vi.advanceTimersByTime(1);
    expect(seen.loaded).toBe(1);
  });

  it('starts only after the page has finished loading', () => {
    vi.useFakeTimers();
    Object.defineProperty(document, 'readyState', { value: 'loading', configurable: true });
    const { seen } = run(marked, () => of('chunk'));

    vi.advanceTimersByTime(60_000);
    expect(seen.loaded).toBe(0);

    window.dispatchEvent(new Event('load'));
    vi.advanceTimersByTime(PRELOAD_DELAY_MS + FALLBACK_IDLE_MS);

    expect(seen.loaded).toBe(1);
  });

  it('loads nothing when the preload is dropped before it starts', () => {
    vi.useFakeTimers();
    const { seen, subscription } = run(marked, () => of('chunk'));
    vi.advanceTimersByTime(PRELOAD_DELAY_MS - 1);
    subscription.unsubscribe();
    vi.advanceTimersByTime(60_000);

    expect(seen.loaded).toBe(0);
  });

  it('cancels the idle wait when the preload is dropped', () => {
    vi.useFakeTimers();
    const cancel = vi.fn();
    (window as unknown as { requestIdleCallback: unknown }).requestIdleCallback = () => 7;
    (window as unknown as { cancelIdleCallback: unknown }).cancelIdleCallback = cancel;

    const { subscription } = run(marked, () => of('chunk'));
    vi.advanceTimersByTime(PRELOAD_DELAY_MS);
    subscription.unsubscribe();

    expect(cancel).toHaveBeenCalledWith(7);
  });

  it('ignores a preload that fails: the page loads the file when it is opened', () => {
    vi.useFakeTimers();
    const { seen } = run(marked, () => throwError(() => new Error('404 chunk')));
    vi.advanceTimersByTime(PRELOAD_DELAY_MS + FALLBACK_IDLE_MS);

    expect(seen.loaded).toBe(1);
    expect(seen.error).toBeUndefined();
    expect(seen.completed).toBe(true);
  });

  it.each([
    ['the user asked to save data', { saveData: true, effectiveType: '4g' }],
    ['the connection is 2G', { effectiveType: '2g' }],
    ['the connection is slow 2G', { effectiveType: 'slow-2g' }],
  ])('loads nothing when %s', (_why, connection) => {
    vi.useFakeTimers();
    setConnection(connection);
    const { seen } = run(marked, () => of('chunk'));
    vi.advanceTimersByTime(60_000);

    expect(seen.loaded).toBe(0);
    expect(seen.completed).toBe(true);
  });

  it('loads on a fast connection', () => {
    vi.useFakeTimers();
    setConnection({ saveData: false, effectiveType: '4g' });
    const { seen } = run(marked, () => of('chunk'));
    vi.advanceTimersByTime(PRELOAD_DELAY_MS + FALLBACK_IDLE_MS);

    expect(seen.loaded).toBe(1);
  });
});
