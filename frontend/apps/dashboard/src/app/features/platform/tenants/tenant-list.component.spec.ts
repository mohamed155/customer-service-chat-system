import { Location } from '@angular/common';
import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { of } from 'rxjs';
import { ApiError, ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import { TenantSummary } from '../../../core/api/tenant-api.models';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { APP_PATHS } from '../../../core/router/app-paths';
import { PlatformTenantsService } from './platform-tenants.service';
import { TenantsStore, TenantListStatus } from './tenants.store';
import { TenantListComponent } from './tenant-list.component';

@Component({ template: '' })
class TenantDetailStubComponent {}

const tenant = (
  id: string,
  name: string,
  slug: string,
  status: 'active' | 'suspended' = 'active',
): TenantSummary => ({
  id,
  name,
  slug,
  status,
  plan: 'starter',
});

interface StoreMock {
  load: ReturnType<typeof vi.fn>;
  setQueryInput: ReturnType<typeof vi.fn>;
  setStatusFilter: ReturnType<typeof vi.fn>;
  resetFilters: ReturnType<typeof vi.fn>;
  loadMore: ReturnType<typeof vi.fn>;
  items: ReturnType<typeof signal<readonly TenantSummary[]>>;
  query: ReturnType<typeof signal<string>>;
  statusFilter: ReturnType<typeof signal<'active' | 'suspended' | null>>;
  status: ReturnType<typeof signal<TenantListStatus>>;
  loading: ReturnType<typeof signal<boolean>>;
  loadingMore: ReturnType<typeof signal<boolean>>;
  hasMore: ReturnType<typeof signal<boolean>>;
  nextCursor: ReturnType<typeof signal<string | null>>;
  error: ReturnType<typeof signal<ApiError | null>>;
  loadMoreError: ReturnType<typeof signal<ApiError | null>>;
}

const createStoreMock = (): StoreMock => ({
  load: vi.fn(),
  setQueryInput: vi.fn(),
  setStatusFilter: vi.fn(),
  resetFilters: vi.fn(),
  loadMore: vi.fn(),
  items: signal<readonly TenantSummary[]>([]),
  query: signal<string>(''),
  statusFilter: signal<'active' | 'suspended' | null>(null),
  status: signal<TenantListStatus>('pending'),
  loading: signal<boolean>(true),
  loadingMore: signal<boolean>(false),
  hasMore: signal<boolean>(false),
  nextCursor: signal<string | null>(null),
  error: signal<ApiError | null>(null),
  loadMoreError: signal<ApiError | null>(null),
});

describe('TenantListComponent', () => {
  let fixture: ComponentFixture<TenantListComponent>;
  let store: StoreMock;
  let permissions: { has: ReturnType<typeof vi.fn> };
  let location: Location;

  async function setup(opts?: { canManage?: boolean }) {
    store = createStoreMock();
    permissions = { has: vi.fn().mockReturnValue(opts?.canManage ?? false) };

    await TestBed.configureTestingModule({
      imports: [TenantListComponent],
      providers: [
        provideRouter([
          {
            path: APP_PATHS.platform.base,
            children: [
              {
                path: APP_PATHS.platform.tenants,
                component: TenantListComponent,
              },
              {
                path: `${APP_PATHS.platform.tenants}/:id`,
                component: TenantDetailStubComponent,
              },
            ],
          },
        ]),
        provideTaiga(),
        { provide: TenantsStore, useValue: store },
        { provide: PermissionsService, useValue: permissions },
      ],
    }).compileComponents();

    location = TestBed.inject(Location);
    fixture = TestBed.createComponent(TenantListComponent);
    fixture.detectChanges();
  }

  it('requests tenants on init', async () => {
    await setup();
    expect(store.load).toHaveBeenCalledTimes(1);
  });

  it('shows the loading state when the store is pending', async () => {
    await setup();
    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('app-empty-state')).toBeNull();
    expect(fixture.nativeElement.querySelector('app-data-table')).toBeNull();
  });

  it('shows the empty state when there are no tenants', async () => {
    await setup();
    store.status.set('empty');
    store.loading.set(false);
    store.items.set([]);
    fixture.detectChanges();

    const empty = fixture.nativeElement.querySelector('app-empty-state');
    expect(empty).toBeTruthy();
    expect(empty.textContent).toContain('No tenants yet');
    expect(fixture.nativeElement.querySelector('app-data-table')).toBeNull();
  });

  it('renders one row per tenant when items are present', async () => {
    await setup();
    store.status.set('success');
    store.loading.set(false);
    store.items.set([
      tenant('t-1', 'Acme Corp', 'acme'),
      tenant('t-2', 'Globex Inc', 'globex', 'suspended'),
    ]);
    fixture.detectChanges();

    const rows = fixture.nativeElement.querySelectorAll('tbody tr');
    expect(rows.length).toBe(2);
    expect(rows[0].textContent).toContain('Acme Corp');
    expect(rows[0].textContent).toContain('acme');
    expect(rows[0].querySelector('app-status-badge')?.textContent?.trim()).toBe('Active');
    expect(rows[1].textContent).toContain('Globex Inc');
    expect(rows[1].textContent).toContain('globex');
    expect(rows[1].querySelector('app-status-badge')?.textContent?.trim()).toBe('Suspended');
    expect(fixture.nativeElement.querySelector('app-empty-state')).toBeNull();
  });

  it('shows an error state with a retry button when the store reports an error', async () => {
    await setup();
    store.status.set('error');
    store.loading.set(false);
    store.error.set({ code: 'internal_error', message: 'boom', status: 500 });
    fixture.detectChanges();

    const empty = fixture.nativeElement.querySelector('app-empty-state');
    expect(empty).toBeTruthy();
    expect(empty.textContent).toContain('Something went wrong');
    const retry = empty.querySelector('button') as HTMLButtonElement;
    expect(retry.textContent).toContain('Try again');
    retry.click();
    expect(store.load).toHaveBeenCalledTimes(2);
  });

  it('hides the New tenant link without platform.tenants.manage', async () => {
    await setup({ canManage: false });
    expect(permissions.has).toHaveBeenCalledWith('platform.tenants.manage');
    expect(fixture.nativeElement.querySelector('.new-link')).toBeNull();
    expect(fixture.nativeElement.textContent).not.toContain('New tenant');
  });

  it('shows the New tenant link when the user has platform.tenants.manage', async () => {
    await setup({ canManage: true });
    const link = fixture.nativeElement.querySelector('.new-link') as HTMLAnchorElement;
    expect(link).toBeTruthy();
    expect(link.textContent).toContain('New tenant');
    expect(link.getAttribute('href')).toBe('/platform/tenants/new');
  });

  describe('search and filter toolbar', () => {
    it('renders a search input and a status filter select', async () => {
      await setup();
      const search = fixture.nativeElement.querySelector('app-search-input input');
      const select = fixture.nativeElement.querySelector('select.status-filter');
      expect(search).toBeTruthy();
      expect(select).toBeTruthy();
      expect(select.querySelectorAll('option').length).toBe(3);
      expect(select.querySelector('option[value=""]').textContent.trim()).toBe('All statuses');
      expect(select.querySelector('option[value="active"]').textContent.trim()).toBe('Active');
      expect(select.querySelector('option[value="suspended"]').textContent.trim()).toBe(
        'Suspended',
      );
    });

    it('forwards the typed value to store.setQueryInput', async () => {
      await setup();
      const input = fixture.nativeElement.querySelector(
        'app-search-input input',
      ) as HTMLInputElement;
      input.value = 'acme';
      input.dispatchEvent(new Event('input'));
      fixture.detectChanges();
      expect(store.setQueryInput).toHaveBeenCalledWith('acme');
    });

    it('forwards the selected status to store.setStatusFilter on change', async () => {
      await setup();
      const select = fixture.nativeElement.querySelector(
        'select.status-filter',
      ) as HTMLSelectElement;
      select.value = 'suspended';
      select.dispatchEvent(new Event('change'));
      fixture.detectChanges();
      expect(store.setStatusFilter).toHaveBeenCalledWith('suspended');
    });

    it('forwards null to store.setStatusFilter when "All statuses" is selected', async () => {
      await setup();
      const select = fixture.nativeElement.querySelector(
        'select.status-filter',
      ) as HTMLSelectElement;
      select.value = 'active';
      select.dispatchEvent(new Event('change'));
      select.value = '';
      select.dispatchEvent(new Event('change'));
      fixture.detectChanges();
      expect(store.setStatusFilter).toHaveBeenLastCalledWith(null);
    });
  });

  describe('unfiltered empty state', () => {
    it('offers a New tenant link inside the empty state when the user has platform.tenants.manage', async () => {
      await setup({ canManage: true });
      store.status.set('empty');
      store.loading.set(false);
      store.items.set([]);
      fixture.detectChanges();

      const empty = fixture.nativeElement.querySelector('app-empty-state') as HTMLElement;
      expect(empty).toBeTruthy();
      expect(empty.textContent).toContain('No tenants yet');
      const link = empty.querySelector('a.primary-button') as HTMLAnchorElement;
      expect(link).toBeTruthy();
      expect(link.textContent).toContain('New tenant');
      expect(link.getAttribute('href')).toBe('/platform/tenants/new');
    });

    it('hides the New tenant link inside the empty state without platform.tenants.manage', async () => {
      await setup({ canManage: false });
      store.status.set('empty');
      store.loading.set(false);
      store.items.set([]);
      fixture.detectChanges();

      const empty = fixture.nativeElement.querySelector('app-empty-state') as HTMLElement;
      expect(empty).toBeTruthy();
      expect(empty.textContent).toContain('No tenants yet');
      expect(empty.querySelector('a.primary-button')).toBeNull();
    });
  });

  describe('search-specific empty state', () => {
    it('shows the no-match state when items are empty and a search query is set', async () => {
      await setup();
      store.status.set('empty');
      store.loading.set(false);
      store.items.set([]);
      const input = fixture.nativeElement.querySelector(
        'app-search-input input',
      ) as HTMLInputElement;
      input.value = 'acme';
      input.dispatchEvent(new Event('input'));
      fixture.detectChanges();

      const empty = fixture.nativeElement.querySelector('app-empty-state');
      expect(empty).toBeTruthy();
      expect(empty.textContent).toContain('No tenants match');
      const clearButton = empty.querySelector('button') as HTMLButtonElement;
      expect(clearButton.textContent).toContain('Clear filters');
    });

    it('shows the no-match state when items are empty and a status filter is set', async () => {
      await setup();
      store.status.set('empty');
      store.loading.set(false);
      store.items.set([]);
      const select = fixture.nativeElement.querySelector(
        'select.status-filter',
      ) as HTMLSelectElement;
      select.value = 'suspended';
      select.dispatchEvent(new Event('change'));
      fixture.detectChanges();

      const empty = fixture.nativeElement.querySelector('app-empty-state');
      expect(empty.textContent).toContain('No tenants match');
    });

    it('Clear filters resets the inputs and triggers a fresh directory load', async () => {
      await setup();
      store.status.set('empty');
      store.loading.set(false);
      store.items.set([]);
      const input = fixture.nativeElement.querySelector(
        'app-search-input input',
      ) as HTMLInputElement;
      const select = fixture.nativeElement.querySelector(
        'select.status-filter',
      ) as HTMLSelectElement;
      input.value = 'acme';
      input.dispatchEvent(new Event('input'));
      select.value = 'suspended';
      select.dispatchEvent(new Event('change'));
      fixture.detectChanges();

      const clearButton = fixture.nativeElement.querySelector(
        'app-empty-state button',
      ) as HTMLButtonElement;
      const resetCallsBefore = store.resetFilters.mock.calls.length;
      const setQueryCallsBefore = store.setQueryInput.mock.calls.length;
      const setStatusCallsBefore = store.setStatusFilter.mock.calls.length;
      clearButton.click();
      fixture.detectChanges();

      expect(input.value).toBe('');
      expect(select.value).toBe('');
      expect(store.resetFilters).toHaveBeenCalledTimes(resetCallsBefore + 1);
      expect(store.setQueryInput).toHaveBeenCalledTimes(setQueryCallsBefore);
      expect(store.setStatusFilter).toHaveBeenCalledTimes(setStatusCallsBefore);
    });
  });

  describe('load more', () => {
    it('shows a Load more button when the store reports more pages', async () => {
      await setup();
      store.status.set('success');
      store.loading.set(false);
      store.items.set([tenant('t-1', 'Acme Corp', 'acme')]);
      store.hasMore.set(true);
      fixture.detectChanges();

      const button = fixture.nativeElement.querySelector('.load-more-button') as HTMLButtonElement;
      expect(button).toBeTruthy();
      expect(button.textContent).toContain('Load more');
    });

    it('invokes store.loadMore when clicked', async () => {
      await setup();
      store.status.set('success');
      store.loading.set(false);
      store.items.set([tenant('t-1', 'Acme Corp', 'acme')]);
      store.hasMore.set(true);
      fixture.detectChanges();

      const button = fixture.nativeElement.querySelector('.load-more-button') as HTMLButtonElement;
      button.click();
      expect(store.loadMore).toHaveBeenCalledTimes(1);
    });

    it('disables the button and shows Loading… while loadingMore is true', async () => {
      await setup();
      store.status.set('loadingMore');
      store.loading.set(false);
      store.items.set([tenant('t-1', 'Acme Corp', 'acme')]);
      store.hasMore.set(true);
      store.loadingMore.set(true);
      fixture.detectChanges();

      const button = fixture.nativeElement.querySelector('.load-more-button') as HTMLButtonElement;
      expect(button.disabled).toBe(true);
      expect(button.textContent).toContain('Loading');
    });

    it('hides the Load more button when hasMore is false', async () => {
      await setup();
      store.status.set('success');
      store.loading.set(false);
      store.items.set([tenant('t-1', 'Acme Corp', 'acme')]);
      store.hasMore.set(false);
      fixture.detectChanges();

      expect(fixture.nativeElement.querySelector('.load-more-button')).toBeNull();
    });

    it('shows an inline error when loadMore fails without replacing the table', async () => {
      await setup();
      store.status.set('success');
      store.loading.set(false);
      store.items.set([tenant('t-1', 'Acme Corp', 'acme')]);
      store.hasMore.set(true);
      store.loadMoreError.set({ code: 'internal_error', message: 'Network failure', status: 500 });
      fixture.detectChanges();

      const rows = fixture.nativeElement.querySelectorAll('tbody tr');
      expect(rows.length).toBe(1);
      expect(rows[0].textContent).toContain('Acme Corp');

      const errorEl = fixture.nativeElement.querySelector('.load-more-error');
      expect(errorEl).toBeTruthy();
      expect(errorEl.textContent).toContain('Network failure');
      expect(errorEl.getAttribute('role')).toBe('alert');
    });
  });

  describe('tenant directory rows', () => {
    it('renders one row per tenant', async () => {
      await setup();
      store.status.set('success');
      store.loading.set(false);
      store.items.set([tenant('t-1', 'Acme Corp', 'acme'), tenant('t-2', 'Globex Inc', 'globex')]);
      fixture.detectChanges();

      const rows = fixture.nativeElement.querySelectorAll('tbody tr');
      expect(rows.length).toBe(2);
    });

    it("renders each row's name cell as an anchor with href to the tenant detail page", async () => {
      await setup();
      store.status.set('success');
      store.loading.set(false);
      store.items.set([tenant('t-1', 'Acme Corp', 'acme'), tenant('t-2', 'Globex Inc', 'globex')]);
      fixture.detectChanges();

      const links = fixture.nativeElement.querySelectorAll(
        'tbody tr td:first-child a',
      ) as NodeListOf<HTMLAnchorElement>;
      expect(links.length).toBe(2);
      expect(links[0].getAttribute('href')).toBe('/platform/tenants/t-1');
      expect(links[1].getAttribute('href')).toBe('/platform/tenants/t-2');
    });

    it('renders the tenant name as the link text', async () => {
      await setup();
      store.status.set('success');
      store.loading.set(false);
      store.items.set([tenant('t-1', 'Acme Corp', 'acme'), tenant('t-2', 'Globex Inc', 'globex')]);
      fixture.detectChanges();

      const links = fixture.nativeElement.querySelectorAll(
        'tbody tr td:first-child a',
      ) as NodeListOf<HTMLAnchorElement>;
      expect(links[0].textContent?.trim()).toBe('Acme Corp');
      expect(links[1].textContent?.trim()).toBe('Globex Inc');
    });

    it("navigates to the tenant detail page when a row's name link is clicked", async () => {
      await setup();
      store.status.set('success');
      store.loading.set(false);
      store.items.set([tenant('t-1', 'Acme Corp', 'acme')]);
      fixture.detectChanges();

      const link = fixture.nativeElement.querySelector(
        'tbody tr td:first-child a',
      ) as HTMLAnchorElement;
      link.click();
      await fixture.whenStable();

      expect(location.path()).toBe('/platform/tenants/t-1');
    });
  });

  describe('list call dedupe (T051)', () => {
    let realFixture: ComponentFixture<TenantListComponent>;
    let service: { list: ReturnType<typeof vi.fn> };

    const emptyPage = (): ApiResponse<PaginatedResponse<TenantSummary>> => ({
      data: { items: [], nextCursor: null, hasMore: false },
    });

    async function setupWithRealStore() {
      service = {
        list: vi.fn().mockReturnValue(of(emptyPage())),
      };

      await TestBed.configureTestingModule({
        imports: [TenantListComponent],
        providers: [
          provideRouter([]),
          provideTaiga(),
          { provide: PlatformTenantsService, useValue: service },
          { provide: PermissionsService, useValue: { has: vi.fn().mockReturnValue(false) } },
        ],
      }).compileComponents();

      realFixture = TestBed.createComponent(TenantListComponent);
      realFixture.detectChanges();
    }

    it('triggers exactly one list call on init and no debounced duplicate from the search effect', async () => {
      vi.useFakeTimers();
      try {
        await setupWithRealStore();

        expect(service.list).toHaveBeenCalledTimes(1);

        vi.advanceTimersByTime(500);

        expect(service.list).toHaveBeenCalledTimes(1);
      } finally {
        vi.useRealTimers();
      }
    });

    it('triggers exactly one additional list call after the user types (post-debounce)', async () => {
      vi.useFakeTimers();
      try {
        await setupWithRealStore();

        const initialCalls = service.list.mock.calls.length;
        expect(initialCalls).toBe(1);

        const input = realFixture.nativeElement.querySelector(
          'app-search-input input',
        ) as HTMLInputElement;
        input.value = 'acme';
        input.dispatchEvent(new Event('input'));
        realFixture.detectChanges();

        expect(service.list).toHaveBeenCalledTimes(initialCalls);

        vi.advanceTimersByTime(500);

        expect(service.list).toHaveBeenCalledTimes(initialCalls + 1);
        expect(service.list).toHaveBeenLastCalledWith({ q: 'acme', limit: 25 });
      } finally {
        vi.useRealTimers();
      }
    });

    it('triggers exactly one list call per clearFilters action (no debounced duplicate from the search effect)', async () => {
      vi.useFakeTimers();
      try {
        await setupWithRealStore();
        vi.advanceTimersByTime(500);

        const input = realFixture.nativeElement.querySelector(
          'app-search-input input',
        ) as HTMLInputElement;
        const select = realFixture.nativeElement.querySelector(
          'select.status-filter',
        ) as HTMLSelectElement;
        input.value = 'acme';
        input.dispatchEvent(new Event('input'));
        realFixture.detectChanges();
        vi.advanceTimersByTime(500);

        select.value = 'suspended';
        select.dispatchEvent(new Event('change'));
        realFixture.detectChanges();
        vi.advanceTimersByTime(500);

        const clearButton = realFixture.nativeElement.querySelector(
          'app-empty-state button',
        ) as HTMLButtonElement;
        expect(clearButton?.textContent).toContain('Clear filters');

        const callsBeforeClear = service.list.mock.calls.length;

        clearButton.click();
        realFixture.detectChanges();

        vi.advanceTimersByTime(500);

        const clearCalls = service.list.mock.calls.length - callsBeforeClear;
        expect(clearCalls).toBe(1);
      } finally {
        vi.useRealTimers();
      }
    });

    it('T064: clearFilters triggers exactly one list call from a non-default filtered state', async () => {
      vi.useFakeTimers();
      try {
        await setupWithRealStore();
        vi.advanceTimersByTime(500);

        const input = realFixture.nativeElement.querySelector(
          'app-search-input input',
        ) as HTMLInputElement;
        const select = realFixture.nativeElement.querySelector(
          'select.status-filter',
        ) as HTMLSelectElement;
        input.value = 'acme';
        input.dispatchEvent(new Event('input'));
        realFixture.detectChanges();
        vi.advanceTimersByTime(500);

        select.value = 'suspended';
        select.dispatchEvent(new Event('change'));
        realFixture.detectChanges();
        vi.advanceTimersByTime(500);

        const clearButton = realFixture.nativeElement.querySelector(
          'app-empty-state button',
        ) as HTMLButtonElement;
        const callsBeforeClear = service.list.mock.calls.length;

        clearButton.click();
        realFixture.detectChanges();
        vi.advanceTimersByTime(500);

        const clearCalls = service.list.mock.calls.length - callsBeforeClear;
        expect(clearCalls).toBe(1);
      } finally {
        vi.useRealTimers();
      }
    });
  });
});
