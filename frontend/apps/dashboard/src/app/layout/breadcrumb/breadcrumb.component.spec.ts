import { Component } from '@angular/core';
import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';

import { BreadcrumbComponent } from './breadcrumb.component';

@Component({ template: '' })
class EmptyComponent {}

describe('BreadcrumbComponent', () => {
  async function setup(routes: Parameters<typeof provideRouter>[0], url: string) {
    TestBed.configureTestingModule({
      providers: [provideZonelessChangeDetection(), provideRouter(routes)],
    });
    await TestBed.compileComponents();
    const router = TestBed.inject(Router);
    await router.navigateByUrl(url);
    const fixture = TestBed.createComponent(BreadcrumbComponent);
    fixture.detectChanges();
    return { fixture };
  }

  it('renders nav landmark with aria-label Breadcrumb', async () => {
    const { fixture } = await setup(
      [
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
      ],
      '/tenant/conversations',
    );

    const nav = (fixture.nativeElement as HTMLElement).querySelector('nav');
    expect(nav).not.toBeNull();
    expect(nav!.getAttribute('aria-label')).toBe('Breadcrumb');
  });

  it('renders crumbs as list items', async () => {
    const { fixture } = await setup(
      [
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
      ],
      '/tenant/conversations',
    );

    const items = (fixture.nativeElement as HTMLElement).querySelectorAll('li');
    expect(items.length).toBe(2);
    expect(items[0].textContent).toContain('Workspace');
    expect(items[1].textContent).toContain('Conversations');
  });

  it('renders link entries with routerLink', async () => {
    const { fixture } = await setup(
      [
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
      ],
      '/tenant/settings',
    );

    const links = (fixture.nativeElement as HTMLElement).querySelectorAll('a');
    expect(links.length).toBe(2);
    expect(links[0].textContent).toContain('Workspace');
    expect(links[1].textContent).toContain('Overview');
  });

  it('renders non-link entries (current page) with aria-current="page"', async () => {
    const { fixture } = await setup(
      [
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
      ],
      '/tenant/conversations',
    );

    const current = (fixture.nativeElement as HTMLElement).querySelector(
      'span[aria-current="page"]',
    );
    expect(current).not.toBeNull();
    expect(current!.textContent).toContain('Conversations');
  });

  it('wraps and truncates on narrow viewports with deep trails', async () => {
    const { fixture } = await setup(
      [
        {
          path: 'a',
          data: { pageTitle: 'Alpha' },
          children: [
            {
              path: 'b',
              data: { pageTitle: 'Bravo' },
              children: [
                {
                  path: 'c',
                  data: { pageTitle: 'Charlie' },
                  children: [
                    {
                      path: 'd',
                      component: EmptyComponent,
                      data: {
                        pageTitle: 'Delta with an extremely long label that should not overflow',
                      },
                    },
                  ],
                },
              ],
            },
          ],
        },
      ],
      '/a/b/c/d',
    );

    const container = (fixture.nativeElement as HTMLElement).querySelector('nav')!;
    container.style.width = '200px';
    const allElements = container.querySelectorAll('*');
    let hasOverflow = false;
    allElements.forEach((el) => {
      if (el.scrollWidth > el.clientWidth) {
        hasOverflow = true;
      }
    });
    expect(hasOverflow).toBe(false);
  });

  it('renders ancestor area-root crumb as navigable anchor when link is set', async () => {
    const { fixture } = await setup(
      [
        {
          path: 'tenant',
          children: [
            {
              path: 'settings',
              component: EmptyComponent,
              data: { pageTitle: 'settings' },
            },
          ],
        },
      ],
      '/tenant/settings',
    );

    const links = (fixture.nativeElement as HTMLElement).querySelectorAll('a');
    const workspaceLink = Array.from(links).find((a) => a.textContent?.trim() === 'Workspace');
    expect(workspaceLink).not.toBeUndefined();
    expect(workspaceLink!.hasAttribute('href')).toBe(true);
  });

  it('aligns breadcrumb inner to content column', async () => {
    const { fixture } = await setup(
      [
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
      ],
      '/tenant/conversations',
    );

    const inner = (fixture.nativeElement as HTMLElement).querySelector('.breadcrumb-inner')!;
    expect(inner).not.toBeNull();
    const style = getComputedStyle(inner);
    expect(style.maxWidth).toBeTruthy();
    expect(style.marginLeft).toBe('auto');
    expect(style.marginRight).toBe('auto');
  });

  it('renders non-link entries (area root) as plain text without aria-current', async () => {
    const { fixture } = await setup(
      [
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
      ],
      '/custom/page',
    );

    const spans = (fixture.nativeElement as HTMLElement).querySelectorAll('span');
    const plainSpans = Array.from(spans).filter((s) => !s.hasAttribute('aria-current'));
    expect(plainSpans.length).toBe(1);
    expect(plainSpans[0].textContent).toContain('Home');
  });
});
