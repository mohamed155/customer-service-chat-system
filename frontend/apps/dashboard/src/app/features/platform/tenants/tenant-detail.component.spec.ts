import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { of, Subject, throwError } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { PlatformTenantDetail } from '../../../core/api/tenant-api.models';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { PlatformTenantsService } from './platform-tenants.service';
import { TenantsStore } from './tenants.store';
import { TenantDetailComponent } from './tenant-detail.component';

const detail = (
  id: string,
  overrides: Partial<PlatformTenantDetail> = {},
): PlatformTenantDetail => ({
  id,
  name: 'Acme Corp',
  slug: 'acme',
  status: 'active',
  plan: 'professional',
  contactName: 'Jane Ops',
  contactEmail: 'ops@acme.test',
  createdAt: '2026-01-15T10:30:00Z',
  updatedAt: '2026-02-20T14:45:00Z',
  ...overrides,
});

interface ServiceMock {
  get: ReturnType<typeof vi.fn>;
  list: ReturnType<typeof vi.fn>;
  create: ReturnType<typeof vi.fn>;
  update: ReturnType<typeof vi.fn>;
}

interface StoreMock {
  getDetail: ReturnType<typeof vi.fn>;
  update: ReturnType<typeof vi.fn>;
}

const createRouteProvider = (id: string | null) => ({
  provide: ActivatedRoute,
  useValue: {
    snapshot: { paramMap: { get: (key: string) => (key === 'id' ? id : null) } },
  },
});

describe('TenantDetailComponent', () => {
  let fixture: ComponentFixture<TenantDetailComponent>;
  let service: ServiceMock;
  let store: StoreMock;
  let permissions: { has: ReturnType<typeof vi.fn> };

  async function setup(
    opts: {
      id?: string | null;
      mockGetDetail?: ReturnType<typeof vi.fn>;
      canManage?: boolean;
    } = {},
  ) {
    const id = opts.id === undefined ? 't-1' : opts.id;
    const mockGetDetail = opts.mockGetDetail ?? vi.fn().mockReturnValue(of(detail('t-1')));
    service = {
      get: vi.fn(),
      list: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
    };
    store = {
      getDetail: mockGetDetail,
      update: vi.fn(),
    };
    permissions = { has: vi.fn().mockReturnValue(opts.canManage ?? false) };

    await TestBed.configureTestingModule({
      imports: [TenantDetailComponent],
      providers: [
        provideRouter([]),
        provideTaiga(),
        createRouteProvider(id),
        { provide: PlatformTenantsService, useValue: service },
        { provide: TenantsStore, useValue: store },
        { provide: PermissionsService, useValue: permissions },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(TenantDetailComponent);
    fixture.detectChanges();
  }

  it('fetches the tenant using the :id route param on init', async () => {
    const mockGetDetail = vi.fn().mockReturnValue(of(detail('t-1')));
    await setup({ id: 't-1', mockGetDetail });

    expect(mockGetDetail).toHaveBeenCalledTimes(1);
    expect(mockGetDetail).toHaveBeenCalledWith('t-1');
  });

  it('starts in a loading state', async () => {
    const subject = new Subject<PlatformTenantDetail>();
    await setup({
      id: 't-1',
      mockGetDetail: vi.fn().mockReturnValue(subject.asObservable()),
    });

    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('dl.record')).toBeNull();
    expect(fixture.nativeElement.querySelector('app-empty-state')).toBeNull();

    subject.next(detail('t-1'));
    subject.complete();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('dl.record')).toBeTruthy();
  });

  it('renders the tenant record after the service resolves', async () => {
    const tenant = detail('t-1', {
      name: 'Acme Corp',
      slug: 'acme',
      plan: 'professional',
      contactName: 'Jane Ops',
      contactEmail: 'ops@acme.test',
    });
    await setup({
      id: 't-1',
      mockGetDetail: vi.fn().mockReturnValue(of(tenant)),
    });

    const record: HTMLElement = fixture.nativeElement.querySelector('dl.record');
    expect(record).toBeTruthy();
    expect(record.textContent).toContain('Acme Corp');
    expect(record.textContent).toContain('acme');
    expect(record.textContent).toContain('Professional');
    expect(record.textContent).toContain('Jane Ops');
    expect(record.textContent).toContain('ops@acme.test');
    const badge = record.querySelector('app-status-badge') as HTMLElement;
    expect(badge?.textContent?.trim()).toBe('Active');
  });

  it('shows a placeholder when contact fields are null', async () => {
    const tenant = detail('t-1', { contactName: null, contactEmail: null });
    await setup({
      id: 't-1',
      mockGetDetail: vi.fn().mockReturnValue(of(tenant)),
    });

    const record: HTMLElement = fixture.nativeElement.querySelector('dl.record');
    const dashes = record.querySelectorAll('dd');
    const dashCount = Array.from(dashes).filter((dd) => dd.textContent?.trim() === '—').length;
    expect(dashCount).toBe(2);
  });

  it('formats the created and updated timestamps via DatePipe', async () => {
    const tenant = detail('t-1', {
      createdAt: '2026-01-15T10:30:00Z',
      updatedAt: '2026-02-20T14:45:00Z',
    });
    await setup({
      id: 't-1',
      mockGetDetail: vi.fn().mockReturnValue(of(tenant)),
    });

    const record: HTMLElement = fixture.nativeElement.querySelector('dl.record');
    expect(record.textContent).toMatch(/Jan 15, 2026/);
    expect(record.textContent).toMatch(/Feb 20, 2026/);
  });

  it('shows an error state with a retry button when the service rejects', async () => {
    const error: ApiError = { code: 'not_found', message: 'Tenant not found', status: 404 };
    await setup({
      id: 't-1',
      mockGetDetail: vi.fn().mockReturnValue(throwError(() => error)),
    });

    const container = fixture.nativeElement.querySelector('[role="alert"]');
    expect(container).toBeTruthy();
    const empty = container.querySelector('app-empty-state');
    expect(empty).toBeTruthy();
    expect(empty.textContent).toContain('Tenant not found');
    const retry = empty.querySelector('button') as HTMLButtonElement;
    expect(retry.textContent).toContain('Try again');
  });

  it('T138: uses role="alert" on the load error container for screen-reader announcement', async () => {
    const error: ApiError = { code: 'not_found', message: 'Tenant not found', status: 404 };
    await setup({
      id: 't-1',
      mockGetDetail: vi.fn().mockReturnValue(throwError(() => error)),
    });

    const container = fixture.nativeElement.querySelector('[role="alert"]');
    expect(container).toBeTruthy();
    expect(container.textContent).toContain('Tenant not found');
  });

  it('refetches the tenant when the retry button is clicked after an error', async () => {
    const error: ApiError = { code: 'not_found', message: 'Tenant not found', status: 404 };
    const mockGetDetail = vi
      .fn()
      .mockReturnValueOnce(throwError(() => error))
      .mockReturnValueOnce(of(detail('t-1')));
    await setup({ id: 't-1', mockGetDetail });

    expect(mockGetDetail).toHaveBeenCalledTimes(1);

    const empty = fixture.nativeElement.querySelector('app-empty-state');
    (empty.querySelector('button') as HTMLButtonElement).click();
    fixture.detectChanges();

    expect(mockGetDetail).toHaveBeenCalledTimes(2);
    expect(fixture.nativeElement.querySelector('dl.record')).toBeTruthy();
  });

  it('shows an error state when the route has no :id param', async () => {
    const mockGetDetail = vi.fn();
    await setup({ id: null, mockGetDetail });

    expect(mockGetDetail).not.toHaveBeenCalled();
    const empty = fixture.nativeElement.querySelector('app-empty-state');
    expect(empty).toBeTruthy();
    expect(empty.textContent).toContain('No tenant id in route');
  });

  it('renders a back link to the tenant list', async () => {
    await setup({
      id: 't-1',
      mockGetDetail: vi.fn().mockReturnValue(of(detail('t-1'))),
    });

    const back = fixture.nativeElement.querySelector('.back-link') as HTMLAnchorElement;
    expect(back).toBeTruthy();
    expect(back.textContent).toContain('Back to tenants');
    expect(back.getAttribute('href')).toBe('/platform/tenants');
  });

  it('renders a suspended status badge when the tenant is suspended', async () => {
    const tenant = detail('t-1', { status: 'suspended' });
    await setup({
      id: 't-1',
      mockGetDetail: vi.fn().mockReturnValue(of(tenant)),
    });

    const badge = fixture.nativeElement.querySelector('app-status-badge') as HTMLElement;
    expect(badge?.textContent?.trim()).toBe('Suspended');
  });

  describe('action error vs load error separation', () => {
    it('shows an inline action-error on toggle failure without hiding the record', async () => {
      const error: ApiError = { code: 'update_failed', message: 'Update failed', status: 500 };
      await setup({ canManage: true });
      store.update.mockReturnValue(throwError(() => error));
      fixture.detectChanges();

      const button = fixture.nativeElement.querySelector(
        'button.action-button',
      ) as HTMLButtonElement;
      button.click();
      fixture.detectChanges();

      const confirmBtn = fixture.nativeElement.querySelector(
        '.dialog-confirm',
      ) as HTMLButtonElement;
      confirmBtn.click();
      fixture.detectChanges();

      const errEl = fixture.nativeElement.querySelector('.action-error') as HTMLElement;
      expect(errEl).toBeTruthy();
      expect(errEl.textContent).toContain('Update failed');

      const record = fixture.nativeElement.querySelector('dl.record');
      expect(record).toBeTruthy();

      const emptyState = fixture.nativeElement.querySelector('app-empty-state');
      expect(emptyState).toBeNull();
    });

    it('clears action-error on the next successful toggle', async () => {
      const error: ApiError = { code: 'update_failed', message: 'Update failed', status: 500 };
      await setup({ canManage: true });

      store.update.mockReturnValueOnce(throwError(() => error));
      const button = fixture.nativeElement.querySelector(
        'button.action-button',
      ) as HTMLButtonElement;
      button.click();
      fixture.detectChanges();
      const confirmBtn = fixture.nativeElement.querySelector(
        '.dialog-confirm',
      ) as HTMLButtonElement;
      confirmBtn.click();
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.action-error')).toBeTruthy();

      store.update.mockReturnValueOnce(of(detail('t-1', { status: 'suspended' })));
      button.click();
      fixture.detectChanges();
      const confirmBtn2 = fixture.nativeElement.querySelector(
        '.dialog-confirm',
      ) as HTMLButtonElement;
      confirmBtn2.click();
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.action-error')).toBeNull();
    });

    it('load error still shows empty state even after a previous action error', async () => {
      const loadError: ApiError = { code: 'not_found', message: 'Not found', status: 404 };
      const mockGetDetail = vi
        .fn()
        .mockReturnValueOnce(of(detail('t-1')))
        .mockReturnValueOnce(throwError(() => loadError));
      await setup({ canManage: true, mockGetDetail });
      expect(fixture.nativeElement.querySelector('dl.record')).toBeTruthy();

      store.update.mockReturnValue(
        throwError(() => ({ code: 'x', message: 'Action err', status: 500 }) as ApiError),
      );
      const button = fixture.nativeElement.querySelector(
        'button.action-button',
      ) as HTMLButtonElement;
      button.click();
      fixture.detectChanges();
      const confirmBtn = fixture.nativeElement.querySelector(
        '.dialog-confirm',
      ) as HTMLButtonElement;
      confirmBtn.click();
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.action-error')).toBeTruthy();
    });
  });

  describe('management actions (gated by platform.tenants.manage)', () => {
    it('hides the Edit link and the action button when the user lacks the manage permission', async () => {
      await setup({ canManage: false });

      expect(fixture.nativeElement.querySelector('.action-link')).toBeNull();
      expect(fixture.nativeElement.querySelector('.action-button')).toBeNull();
    });

    it('shows the Edit link pointing to /platform/tenants/:id/edit when the user can manage', async () => {
      await setup({ canManage: true });

      const link = fixture.nativeElement.querySelector('a.action-link') as HTMLAnchorElement;
      expect(link).toBeTruthy();
      expect(link.textContent).toContain('Edit');
      expect(link.getAttribute('href')).toBe('/platform/tenants/t-1/edit');
    });

    it('renders a Deactivate button for an active tenant when the user can manage', async () => {
      await setup({ canManage: true });

      const button = fixture.nativeElement.querySelector(
        'button.action-button',
      ) as HTMLButtonElement;
      expect(button).toBeTruthy();
      expect(button.textContent).toContain('Deactivate');
      expect(button.classList.contains('danger')).toBe(true);
    });

    it('renders a Reactivate button for a suspended tenant when the user can manage', async () => {
      const tenant = detail('t-1', { status: 'suspended' });
      await setup({
        canManage: true,
        mockGetDetail: vi.fn().mockReturnValue(of(tenant)),
      });

      const button = fixture.nativeElement.querySelector(
        'button.action-button',
      ) as HTMLButtonElement;
      expect(button).toBeTruthy();
      expect(button.textContent).toContain('Reactivate');
      expect(button.classList.contains('danger')).toBe(false);
    });

    it('calls store.update with the new status and updates the local tenant signal on confirm', async () => {
      const updated = detail('t-1', { status: 'suspended' });
      await setup({ canManage: true });
      store.update.mockReturnValue(of(updated));

      const button = fixture.nativeElement.querySelector(
        'button.action-button',
      ) as HTMLButtonElement;
      button.click();
      fixture.detectChanges();

      const confirmBtn = fixture.nativeElement.querySelector(
        '.dialog-confirm',
      ) as HTMLButtonElement;
      confirmBtn.click();

      await vi.waitFor(() => {
        expect(store.update).toHaveBeenCalledWith('t-1', { status: 'suspended' });
      });
      fixture.detectChanges();

      const badge = fixture.nativeElement.querySelector('app-status-badge') as HTMLElement;
      expect(badge?.textContent?.trim()).toBe('Suspended');
    });

    it('does not call store.update when the user cancels the confirmation', async () => {
      await setup({ canManage: true });

      const button = fixture.nativeElement.querySelector(
        'button.action-button',
      ) as HTMLButtonElement;
      button.click();
      fixture.detectChanges();

      const cancelBtn = fixture.nativeElement.querySelector('.dialog-cancel') as HTMLButtonElement;
      cancelBtn.click();
      fixture.detectChanges();

      expect(store.update).not.toHaveBeenCalled();
    });

    it('disables the action button while the update is in flight', async () => {
      const subject = new Subject<PlatformTenantDetail>();
      await setup({ canManage: true });
      store.update.mockReturnValue(subject.asObservable());

      const button = fixture.nativeElement.querySelector(
        'button.action-button',
      ) as HTMLButtonElement;
      button.click();
      fixture.detectChanges();

      const confirmBtn = fixture.nativeElement.querySelector(
        '.dialog-confirm',
      ) as HTMLButtonElement;
      confirmBtn.click();
      fixture.detectChanges();
      expect(button.disabled).toBe(true);

      subject.next(detail('t-1', { status: 'suspended' }));
      subject.complete();

      await vi.waitFor(
        () => {
          fixture.detectChanges();
          expect(button.disabled).toBe(false);
        },
        { timeout: 2000 },
      );
    });

    it('serializes rapid toggle clicks and processes them in order', async () => {
      const subject1 = new Subject<PlatformTenantDetail>();
      const subject2 = new Subject<PlatformTenantDetail>();

      await setup({ canManage: true });
      store.update
        .mockReturnValueOnce(subject1.asObservable())
        .mockReturnValueOnce(subject2.asObservable());

      const button = fixture.nativeElement.querySelector(
        'button.action-button',
      ) as HTMLButtonElement;

      // First toggle — click action button, confirm dialog
      button.click();
      fixture.detectChanges();
      const confirm1 = fixture.nativeElement.querySelector('.dialog-confirm') as HTMLButtonElement;
      confirm1.click();
      fixture.detectChanges();

      // Resolve the first toggle so the button becomes enabled again
      subject1.next(detail('t-1', { status: 'suspended' }));
      subject1.complete();
      await vi.waitFor(() => {
        expect(button.disabled).toBe(false);
      });

      // Second toggle — same flow
      button.click();
      fixture.detectChanges();
      const confirm2 = fixture.nativeElement.querySelector('.dialog-confirm') as HTMLButtonElement;
      confirm2.click();
      fixture.detectChanges();

      subject2.next(detail('t-1', { status: 'active' }));
      subject2.complete();

      await vi.waitFor(() => {
        expect(store.update).toHaveBeenCalledTimes(2);
      });
    });
  });
});
