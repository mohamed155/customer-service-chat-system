import { DOCUMENT } from '@angular/common';
import { TestBed } from '@angular/core/testing';
import {
  AppUpdateService,
  releaseFingerprint,
  UPDATE_CHECK_INTERVAL_MS,
  UPDATE_CHECK_MIN_GAP_MS,
} from './app-update.service';
import { PageReloader } from './page-reloader';

const page = (main: string, styles = 'styles-QWERTY12.css') => `<!doctype html><html><head>
  <script src="theme-init.js"></script>
  <link rel="stylesheet" href="${styles}">
  </head><body><app-root></app-root>
  <script src="polyfills-ABCD1234.js" type="module"></script>
  <script src="${main}" type="module"></script></body></html>`;

describe('releaseFingerprint', () => {
  it('counts chunks with mixed-case hashes, one of them starting with a dash', () => {
    const html =
      '<link rel="modulepreload" href="chunk-54ZL8lHj.js"><script src="chunk--HMCNCmS.js"></script>';

    expect(releaseFingerprint(html)).toBe('chunk--HMCNCmS.js|chunk-54ZL8lHj.js');
  });

  it('lists the hashed files in a fixed order and ignores the unhashed ones', () => {
    expect(releaseFingerprint(page('main-AAAAAAAA.js'))).toBe(
      'main-AAAAAAAA.js|polyfills-ABCD1234.js|styles-QWERTY12.css'.split('|').sort().join('|'),
    );
  });

  it('is the same whatever the order of the tags', () => {
    expect(
      releaseFingerprint('<script src="b-AAAAAAAA.js"></script><link href="a-BBBBBBBB.css">'),
    ).toBe(releaseFingerprint('<link href="a-BBBBBBBB.css"><script src="b-AAAAAAAA.js"></script>'));
  });

  it('differs when a file is rebuilt', () => {
    expect(releaseFingerprint(page('main-AAAAAAAA.js'))).not.toBe(
      releaseFingerprint(page('main-BBBBBBBB.js')),
    );
  });

  it('is null for a page without hashed files', () => {
    expect(releaseFingerprint('<script src="main.js"></script>')).toBeNull();
    expect(releaseFingerprint('')).toBeNull();
  });
});

describe('AppUpdateService', () => {
  let reloader: { reload: ReturnType<typeof vi.fn> };
  let fetchMock: ReturnType<typeof vi.fn>;
  let hidden: boolean;
  let runningPage: string;

  beforeEach(() => {
    vi.useFakeTimers();
    reloader = { reload: vi.fn() };
    hidden = false;
    runningPage = page('main-AAAAAAAA.js');
    fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    const respond = (html: string, ok = true) =>
      fetchMock.mockResolvedValue({ ok, text: () => Promise.resolve(html) });
    respond(runningPage);
    TestBed.configureTestingModule({
      providers: [
        { provide: PageReloader, useValue: reloader },
        {
          provide: DOCUMENT,
          useFactory: () => {
            const doc = document.implementation.createHTMLDocument('app');
            doc.documentElement.innerHTML = runningPage.replace(
              /^<!doctype html><html>|<\/html>$/g,
              '',
            );
            Object.defineProperty(doc, 'hidden', { get: () => hidden });
            Object.defineProperty(doc, 'defaultView', { value: window });
            Object.defineProperty(doc, 'baseURI', { value: 'http://localhost/' });
            return doc;
          },
        },
      ],
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  const newRelease = (html: string, ok = true) =>
    fetchMock.mockResolvedValue({ ok, text: () => Promise.resolve(html) });
  const tick = (ms: number) => vi.advanceTimersByTimeAsync(ms);

  it('stays quiet while the server still serves the release that is running', async () => {
    const service = TestBed.inject(AppUpdateService);
    service.start();

    await tick(UPDATE_CHECK_INTERVAL_MS);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][1]).toMatchObject({ cache: 'no-store', credentials: 'omit' });
    expect(service.updateAvailable()).toBe(false);
  });

  it('offers a reload once the server serves different files', async () => {
    const service = TestBed.inject(AppUpdateService);
    service.start();
    newRelease(page('main-BBBBBBBB.js'));

    await tick(UPDATE_CHECK_INTERVAL_MS);

    expect(service.updateAvailable()).toBe(true);
    expect(service.bannerVisible()).toBe(true);
    service.reload();
    expect(reloader.reload).toHaveBeenCalledWith();
  });

  it('stops asking once an update is known', async () => {
    const service = TestBed.inject(AppUpdateService);
    service.start();
    newRelease(page('main-BBBBBBBB.js'));

    await tick(UPDATE_CHECK_INTERVAL_MS * 3);

    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('does not check while the tab is hidden', async () => {
    const service = TestBed.inject(AppUpdateService);
    service.start();
    hidden = true;

    await tick(UPDATE_CHECK_INTERVAL_MS * 2);

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it.each([
    [
      'a server error',
      () => fetchMock.mockResolvedValue({ ok: false, text: () => Promise.resolve('') }),
    ],
    ['a network failure', () => fetchMock.mockRejectedValue(new TypeError('Failed to fetch'))],
    ['an answer that is not the app page', () => newRelease('<h1>Bad gateway</h1>')],
  ])('ignores %s', async (_what, arrange) => {
    const service = TestBed.inject(AppUpdateService);
    service.start();
    arrange();

    await tick(UPDATE_CHECK_INTERVAL_MS);

    expect(service.updateAvailable()).toBe(false);
  });

  it('checks when the tab returns to the foreground, at most once a minute', async () => {
    const service = TestBed.inject(AppUpdateService);
    const doc = TestBed.inject(DOCUMENT);
    service.start();
    newRelease(page('main-BBBBBBBB.js'));

    await tick(UPDATE_CHECK_MIN_GAP_MS + 1);
    doc.dispatchEvent(new Event('visibilitychange'));
    await tick(0);

    expect(service.updateAvailable()).toBe(true);
  });

  it('does not check on a quick return to the foreground', async () => {
    const service = TestBed.inject(AppUpdateService);
    const doc = TestBed.inject(DOCUMENT);
    service.start();
    await service.check(releaseFingerprint(runningPage)!); // a check just ran: same release
    expect(service.updateAvailable()).toBe(false);
    fetchMock.mockClear();

    doc.dispatchEvent(new Event('visibilitychange'));
    await tick(0);

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('does nothing in a build without hashed files', async () => {
    runningPage = '<script src="main.js"></script>';
    const service = TestBed.inject(AppUpdateService);
    service.start();

    await tick(UPDATE_CHECK_INTERVAL_MS * 2);

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('keeps the notice hidden after a dismiss until it is marked again', () => {
    const service = TestBed.inject(AppUpdateService);
    service.markUpdateAvailable();
    service.dismiss();
    expect(service.bannerVisible()).toBe(false);

    service.markUpdateAvailable();
    expect(service.bannerVisible()).toBe(true);
  });

  it('stops its timer and listener when destroyed', async () => {
    const service = TestBed.inject(AppUpdateService);
    service.start();
    TestBed.resetTestingModule();

    await tick(UPDATE_CHECK_INTERVAL_MS * 2);

    expect(fetchMock).not.toHaveBeenCalled();
    expect(service).toBeDefined();
  });
});
