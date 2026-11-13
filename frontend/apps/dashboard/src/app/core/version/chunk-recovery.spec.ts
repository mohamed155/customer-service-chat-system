import { Location } from '@angular/common';
import { TestBed } from '@angular/core/testing';
import { NavigationError } from '@angular/router';
import { AppUpdateService } from './app-update.service';
import {
  CHUNK_RELOAD_KEY,
  CHUNK_RELOAD_WINDOW_MS,
  recoverFromChunkLoadError,
} from './chunk-recovery';
import { PageReloader } from './page-reloader';

const chunkError = () => new TypeError('Failed to fetch dynamically imported module: /x.js');
const navigationError = (error: unknown, url = '/tenant/customers') =>
  new NavigationError(1, url, error);

describe('recoverFromChunkLoadError', () => {
  let reloader: { reload: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    sessionStorage.clear();
    reloader = { reload: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        { provide: PageReloader, useValue: reloader },
        { provide: Location, useValue: { prepareExternalUrl: (url: string) => url } },
      ],
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  const run = (error: NavigationError) =>
    TestBed.runInInjectionContext(() => recoverFromChunkLoadError(error));

  it('loads the target URL again when a chunk fails to load', () => {
    run(navigationError(chunkError()));

    expect(reloader.reload).toHaveBeenCalledExactlyOnceWith('/tenant/customers');
    expect(TestBed.inject(AppUpdateService).updateAvailable()).toBe(false);
  });

  it('applies the base href to the target URL', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [
        { provide: PageReloader, useValue: reloader },
        { provide: Location, useValue: { prepareExternalUrl: (url: string) => `/app${url}` } },
      ],
    });

    run(navigationError(chunkError(), '/tenant/customers'));

    expect(reloader.reload).toHaveBeenCalledWith('/app/tenant/customers');
  });

  it('does not reload for any other navigation error', () => {
    run(navigationError(new Error('Cannot read properties of undefined')));

    expect(reloader.reload).not.toHaveBeenCalled();
    expect(sessionStorage.getItem(CHUNK_RELOAD_KEY)).toBeNull();
  });

  it('reloads only once: a second failure right after shows the update notice instead', () => {
    run(navigationError(chunkError()));
    run(navigationError(chunkError()));

    expect(reloader.reload).toHaveBeenCalledTimes(1);
    expect(TestBed.inject(AppUpdateService).updateAvailable()).toBe(true);
  });

  it('reloads again for a failure long after the last reload (a later deploy)', () => {
    vi.useFakeTimers();
    run(navigationError(chunkError()));
    vi.advanceTimersByTime(CHUNK_RELOAD_WINDOW_MS + 1);
    run(navigationError(chunkError()));

    expect(reloader.reload).toHaveBeenCalledTimes(2);
  });

  it('does not reload without a usable guard: it shows the notice', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new DOMException('blocked', 'SecurityError');
    });

    run(navigationError(chunkError()));

    expect(reloader.reload).not.toHaveBeenCalled();
    expect(TestBed.inject(AppUpdateService).updateAvailable()).toBe(true);
  });

  it('ignores a corrupt guard record', () => {
    sessionStorage.setItem(CHUNK_RELOAD_KEY, '{not json');

    run(navigationError(chunkError()));

    // A record that cannot be read is treated as no guard (the write below would also fail on a
    // blocked store), so the person gets the notice rather than a loop.
    expect(reloader.reload).not.toHaveBeenCalled();
    expect(TestBed.inject(AppUpdateService).updateAvailable()).toBe(true);
  });
});
