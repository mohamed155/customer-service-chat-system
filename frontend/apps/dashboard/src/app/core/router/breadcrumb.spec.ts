import { Component } from '@angular/core';
import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { Crumb, injectBreadcrumbs } from './breadcrumb';

@Component({ template: '' })
class EmptyComponent {}

describe('injectBreadcrumbs', () => {
  async function setup(routes: Parameters<typeof provideRouter>[0]) {
    TestBed.configureTestingModule({
      providers: [provideZonelessChangeDetection(), provideRouter(routes)],
    });
    await TestBed.compileComponents();
    const router = TestBed.inject(Router);
    return { router };
  }

  it('produces Workspace / Conversations for /tenant/conversations', async () => {
    const { router } = await setup([
      {
        path: 'tenant',
        children: [
          {
            path: 'conversations',
            component: EmptyComponent,
            data: { pageTitle: 'conversations' },
          },
        ],
      },
    ]);

    await router.navigateByUrl('/tenant/conversations');
    const crumbs = TestBed.runInInjectionContext(() => injectBreadcrumbs());

    const expected: Crumb[] = [
      { label: 'Workspace', link: null },
      { label: 'Conversations', link: null },
    ];
    expect(crumbs()).toEqual(expected);
  });

  it('sets area root to Platform when first segment is platform', async () => {
    const { router } = await setup([
      {
        path: 'platform',
        children: [
          {
            path: 'page',
            component: EmptyComponent,
            data: { pageTitle: 'conversations' },
          },
        ],
      },
    ]);

    await router.navigateByUrl('/platform/page');
    const crumbs = TestBed.runInInjectionContext(() => injectBreadcrumbs());

    const expected: Crumb[] = [
      { label: 'Platform', link: null },
      { label: 'Conversations', link: null },
    ];
    expect(crumbs()).toEqual(expected);
  });

  it('sets area root to Home for unknown first segment', async () => {
    const { router } = await setup([
      {
        path: 'custom',
        children: [
          {
            path: 'page',
            component: EmptyComponent,
            data: { pageTitle: 'overview' },
          },
        ],
      },
    ]);

    await router.navigateByUrl('/custom/page');
    const crumbs = TestBed.runInInjectionContext(() => injectBreadcrumbs());

    const expected: Crumb[] = [
      { label: 'Home', link: null },
      { label: 'Overview', link: null },
    ];
    expect(crumbs()).toEqual(expected);
  });

  it('returns single area-root crumb when route has no pageTitle data', async () => {
    const { router } = await setup([
      {
        path: 'no-title',
        component: EmptyComponent,
      },
    ]);

    await router.navigateByUrl('/no-title');
    const crumbs = TestBed.runInInjectionContext(() => injectBreadcrumbs());

    expect(crumbs()).toEqual([{ label: 'Home', link: null }]);
  });

  it('forces the final crumb link to null', async () => {
    const { router } = await setup([
      {
        path: 'tenant',
        data: { pageTitle: 'overview' },
        children: [
          {
            path: 'settings',
            component: EmptyComponent,
            data: { pageTitle: 'settings' },
          },
        ],
      },
    ]);

    await router.navigateByUrl('/tenant/settings');
    const crumbs = TestBed.runInInjectionContext(() => injectBreadcrumbs());

    const last = crumbs().at(-1);
    expect(last?.link).toBeNull();
  });

  it('provides an accumulated link for intermediate crumbs', async () => {
    const { router } = await setup([
      {
        path: 'tenant',
        data: { pageTitle: 'overview' },
        children: [
          {
            path: 'settings',
            component: EmptyComponent,
            data: { pageTitle: 'settings' },
          },
        ],
      },
    ]);

    await router.navigateByUrl('/tenant/settings');
    const crumbs = TestBed.runInInjectionContext(() => injectBreadcrumbs());

    const intermediate = crumbs().at(1);
    expect(intermediate?.link).toBe('/tenant');
  });
});
