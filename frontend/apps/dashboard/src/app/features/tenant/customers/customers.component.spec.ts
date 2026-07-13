import { provideZonelessChangeDetection, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { APP_CONFIG } from '../../../core/config/app-config';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { TenantContextService } from '../../../core/tenant/tenant-context.service';
import { ApiError } from '../../../core/api/api.models';
import { Customer } from '../../../core/api/tenant-api.models';
import { CustomersComponent } from './customers.component';
import { CustomersStore } from './customers.store';

const customer = (id: string, displayName: string): Customer => ({
  id,
  displayName,
  email: `${id}@example.test`,
  phone: '+1 555 0100',
  channels: ['email', 'phone', 'web_chat'],
  createdAt: '2026-07-13T10:00:00Z',
  updatedAt: '2026-07-13T10:00:00Z',
});

describe('CustomersComponent', () => {
  const items = signal<readonly Customer[]>([]);
  const query = signal('');
  const status = signal<'pending' | 'loading' | 'success' | 'empty' | 'error'>('loading');
  const hasMore = signal(false);
  const error = signal<ApiError | null>(null);
  const loadMoreError = signal<ApiError | null>(null);
  const search = vi.fn();
  const loadMore = vi.fn();
  const retry = vi.fn();

  const store = { items, query, status, hasMore, error, loadMoreError, search, loadMore, retry };
  const mockPermissions = { has: vi.fn().mockReturnValue(true) };
  const mockCurrentUserService = {
    currentUser: vi.fn().mockReturnValue({
      id: 'current-user',
      memberships: [],
    }),
  };
  const mockTenantContextService = {
    activeTenant: vi
      .fn()
      .mockReturnValue({ id: 'tenant-1', name: 'Test', slug: 'test', status: 'active' }),
  };

  beforeEach(() => {
    items.set([]);
    query.set('');
    status.set('loading');
    hasMore.set(false);
    error.set(null);
    loadMoreError.set(null);
    search.mockReset();
    loadMore.mockReset();
    retry.mockReset();
    mockPermissions.has.mockReturnValue(true);

    TestBed.configureTestingModule({
      imports: [CustomersComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideRouter([]),
        {
          provide: APP_CONFIG,
          useValue: {
            apiBaseUrl: 'http://localhost:8080/api/v1',
            publicDashboardUrl: 'https://dashboard.example.com',
          },
        },
        { provide: CustomersStore, useValue: store },
        { provide: PermissionsService, useValue: mockPermissions },
        { provide: CurrentUserService, useValue: mockCurrentUserService },
        { provide: TenantContextService, useValue: mockTenantContextService },
      ],
    });
  });

  it('renders live customer name, contact, channels, and profile navigation', async () => {
    items.set([customer('customer-1', 'Sara Ali')]);
    status.set('success');
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Sara Ali');
    expect(text).toContain('customer-1@example.test');
    expect(text).toContain('+1 555 0100');
    expect(fixture.nativeElement.querySelectorAll('app-channel-badge')).toHaveLength(3);
    const link = fixture.nativeElement.querySelector('a.name-link') as HTMLAnchorElement | null;
    expect(link).not.toBeNull();
    expect(link?.getAttribute('href')).toBe('/tenant/customers/customer-1');
  });

  it('passes search input changes to the debounced store search', async () => {
    items.set([customer('customer-1', 'Sara Ali')]);
    status.set('success');
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();

    const input = fixture.nativeElement.querySelector('input') as HTMLInputElement;
    input.value = 'sara';
    input.dispatchEvent(new Event('input'));

    expect(search).toHaveBeenCalledWith('sara');
  });

  it('renders no-results and clears its store search', async () => {
    query.set('missing');
    status.set('empty');
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('No customers match');
    const clearButton = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((button) => button.textContent?.trim() === 'Clear search')!;
    clearButton.click();

    expect(search).toHaveBeenCalledWith('');
  });

  it('shows only the loading state (not an empty data table) when the store is pending/loading with no items', async () => {
    // Initial mount: status is 'pending' with no items rendered yet.
    status.set('pending');
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('app-data-table')).toBeNull();

    // Tenant-switch reload: status is 'loading' with no items rendered yet.
    status.set('loading');
    items.set([]);
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('app-data-table')).toBeNull();
  });

  describe('permissions', () => {
    it('hides the create button when viewer lacks customers.manage', async () => {
      mockPermissions.has.mockReturnValue(false);
      items.set([customer('customer-1', 'Sara Ali')]);
      status.set('success');
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(CustomersComponent);
      fixture.detectChanges();

      const createBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'New customer');
      expect(createBtn).toBeUndefined();
    });

    it('shows the create button when agent has customers.manage', async () => {
      mockPermissions.has.mockReturnValue(true);
      items.set([customer('customer-1', 'Sara Ali')]);
      status.set('success');
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(CustomersComponent);
      fixture.detectChanges();

      const createBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'New customer');
      expect(createBtn).toBeTruthy();
    });

    it('opens create dialog when clicking New customer', async () => {
      mockPermissions.has.mockReturnValue(true);
      items.set([customer('customer-1', 'Sara Ali')]);
      status.set('success');
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(CustomersComponent);
      fixture.detectChanges();

      const createBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'New customer')!;
      createBtn.click();
      fixture.detectChanges();

      expect(fixture.nativeElement.querySelector('app-customer-dialog')).toBeTruthy();
    });
  });

  it('renders and retries a failed continuation without hiding current results', async () => {
    items.set([customer('customer-1', 'Sara Ali')]);
    status.set('success');
    hasMore.set(true);
    loadMoreError.set({ code: 'network_error', message: 'Connection lost', status: 0 });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Sara Ali');
    expect(fixture.nativeElement.textContent).toContain('Connection lost');
    const loadMoreButton = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((button) => button.textContent?.trim() === 'Load more')!;
    loadMoreButton.click();

    expect(loadMore).toHaveBeenCalledOnce();
  });
});
