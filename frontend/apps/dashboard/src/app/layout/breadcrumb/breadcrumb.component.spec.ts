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
    expect(links.length).toBe(1);
    expect(links[0].textContent).toContain('Overview');
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

    const current = (fixture.nativeElement as HTMLElement).querySelector('span[aria-current="page"]');
    expect(current).not.toBeNull();
    expect(current!.textContent).toContain('Conversations');
  });

  it('renders non-link entries (area root) as plain text without aria-current', async () => {
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

    const spans = (fixture.nativeElement as HTMLElement).querySelectorAll('span');
    const plainSpans = Array.from(spans).filter(s => !s.hasAttribute('aria-current'));
    expect(plainSpans.length).toBe(1);
    expect(plainSpans[0].textContent).toContain('Workspace');
  });
});
