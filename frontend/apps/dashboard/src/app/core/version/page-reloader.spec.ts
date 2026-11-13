import { DOCUMENT } from '@angular/common';
import { TestBed } from '@angular/core/testing';
import { PageReloader } from './page-reloader';

describe('PageReloader', () => {
  let location: {
    href: string;
    assign: ReturnType<typeof vi.fn>;
    reload: ReturnType<typeof vi.fn>;
  };
  let history: { state: unknown; replaceState: ReturnType<typeof vi.fn> };
  let reloader: PageReloader;

  function setup(href: string, view = true) {
    location = { href, assign: vi.fn(), reload: vi.fn() };
    history = { state: { n: 1 }, replaceState: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        {
          provide: DOCUMENT,
          useValue: { defaultView: view ? { location, history } : null },
        },
      ],
    });
    reloader = TestBed.inject(PageReloader);
  }

  it('reloads the current page when there is no URL', () => {
    setup('http://localhost/tenant/overview');

    reloader.reload();

    expect(location.reload).toHaveBeenCalledTimes(1);
    expect(location.assign).not.toHaveBeenCalled();
  });

  it('loads another URL as a new page', () => {
    setup('http://localhost/tenant/overview');

    reloader.reload('/tenant/customers');

    expect(location.assign).toHaveBeenCalledExactlyOnceWith('http://localhost/tenant/customers');
    expect(location.reload).not.toHaveBeenCalled();
  });

  it('loads the same URL again (no fragment) as a new page', () => {
    setup('http://localhost/tenant/customers');

    reloader.reload('/tenant/customers');

    expect(location.assign).toHaveBeenCalledExactlyOnceWith('http://localhost/tenant/customers');
  });

  it.each([
    ['the same fragment', 'http://localhost/tenant/customers#details'],
    ['no fragment yet', 'http://localhost/tenant/customers'],
    ['another fragment', 'http://localhost/tenant/customers#other'],
  ])(
    'reloads the document, keeping the target fragment, when the page has %s',
    (_what, current) => {
      setup(current);

      reloader.reload('/tenant/customers#details');

      // `assign` would only jump to the fragment inside the page that failed to load.
      expect(location.assign).not.toHaveBeenCalled();
      expect(history.replaceState).toHaveBeenCalledExactlyOnceWith(
        { n: 1 },
        '',
        'http://localhost/tenant/customers#details',
      );
      expect(location.reload).toHaveBeenCalledTimes(1);
    },
  );

  it('reloads the document for a target that ends in an empty fragment', () => {
    setup('http://localhost/tenant/customers');

    reloader.reload('/tenant/customers#');

    expect(location.assign).not.toHaveBeenCalled();
    expect(history.replaceState).toHaveBeenCalledExactlyOnceWith(
      { n: 1 },
      '',
      'http://localhost/tenant/customers#',
    );
    expect(location.reload).toHaveBeenCalledTimes(1);
  });

  it('does nothing without a window', () => {
    setup('http://localhost/', false);

    expect(() => reloader.reload('/x')).not.toThrow();
  });
});
