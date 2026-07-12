import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, provideRouter, Router } from '@angular/router';
import { of, Subject, throwError } from 'rxjs';
import { ApiError, ApiErrorDetail, ApiResponse } from '../../../core/api/api.models';
import { PlatformTenantDetail } from '../../../core/api/tenant-api.models';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { APP_PATHS } from '../../../core/router/app-paths';
import { PlatformTenantsService } from './platform-tenants.service';
import { TenantsStore } from './tenants.store';
import { TenantFormComponent } from './tenant-form.component';

const detail = (name: string, slug: string): PlatformTenantDetail => ({
  id: 't-new',
  name,
  slug,
  status: 'active',
  plan: 'professional',
  contactName: null,
  contactEmail: null,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
});

const editModeDetail = (): PlatformTenantDetail => ({
  id: 't-1',
  name: 'Acme Corp',
  slug: 'acme',
  status: 'active',
  plan: 'professional',
  contactName: 'Jane Ops',
  contactEmail: 'ops@acme.test',
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
});

describe('TenantFormComponent', () => {
  let fixture: ComponentFixture<TenantFormComponent>;
  let store: {
    create: ReturnType<typeof vi.fn>;
    update: ReturnType<typeof vi.fn>;
  };
  let service: { get: ReturnType<typeof vi.fn> };
  let permissions: { has: ReturnType<typeof vi.fn> };
  let router: Router;

  beforeEach(async () => {
    store = { create: vi.fn(), update: vi.fn() };
    service = { get: vi.fn() };
    permissions = { has: vi.fn().mockReturnValue(true) };

    await TestBed.configureTestingModule({
      imports: [TenantFormComponent],
      providers: [
        provideRouter([]),
        { provide: TenantsStore, useValue: store },
        { provide: PlatformTenantsService, useValue: service },
        { provide: PermissionsService, useValue: permissions },
      ],
    }).compileComponents();

    router = TestBed.inject(Router);
    vi.spyOn(router, 'navigate').mockResolvedValue(true);
    fixture = TestBed.createComponent(TenantFormComponent);
    fixture.detectChanges();
  });

  it('renders the page header and all fields', () => {
    const element: HTMLElement = fixture.nativeElement;
    expect(element.querySelector('h1')?.textContent).toContain('New tenant');
    expect(element.querySelector('input[formControlName="name"]')).toBeTruthy();
    expect(element.querySelector('input[formControlName="slug"]')).toBeTruthy();
    expect(element.querySelector('select[formControlName="plan"]')).toBeTruthy();
    expect(element.querySelector('input[formControlName="contactName"]')).toBeTruthy();
    expect(element.querySelector('input[formControlName="contactEmail"]')).toBeTruthy();
  });

  describe('validation', () => {
    it('marks the name control invalid when empty', () => {
      const control = fixture.componentInstance.form.controls.name;
      control.setValue('');
      control.markAsTouched();
      fixture.detectChanges();
      expect(control.errors).toEqual({ required: true });
      expect(fixture.nativeElement.textContent).toContain('This field is required');
    });

    it('rejects slugs with uppercase letters or spaces', () => {
      const control = fixture.componentInstance.form.controls.slug;
      control.setValue('Acme Inc');
      control.markAsTouched();
      fixture.detectChanges();
      expect(control.errors?.['pattern']?.['actualValue']).toBe('Acme Inc');
      expect(typeof control.errors?.['pattern']?.['requiredPattern']).toBe('string');
      expect(fixture.nativeElement.textContent).toContain('Slug must be lowercase');
    });

    it('accepts lowercase slugs with single hyphens', () => {
      const control = fixture.componentInstance.form.controls.slug;
      control.setValue('acme-corp-1');
      expect(control.errors).toBeNull();
    });

    it('flags the name control when over the 200-character limit', () => {
      const control = fixture.componentInstance.form.controls.name;
      control.setValue('a'.repeat(201));
      control.markAsTouched();
      fixture.detectChanges();
      expect(control.errors).toEqual({ maxlength: { requiredLength: 200, actualLength: 201 } });
      expect(fixture.nativeElement.textContent).toContain('Value is too long');
    });

    it('rejects malformed contact emails but allows empty ones', () => {
      const control = fixture.componentInstance.form.controls.contactEmail;
      control.setValue('not-an-email');
      control.markAsTouched();
      fixture.detectChanges();
      expect(control.errors).toBeTruthy();
      expect(fixture.nativeElement.textContent).toContain('Enter a valid email address');

      control.setValue('');
      expect(control.errors).toBeNull();
    });

    it('disables the submit button while the form is invalid', () => {
      const button = fixture.nativeElement.querySelector(
        'button[type="submit"]',
      ) as HTMLButtonElement;
      expect(button.disabled).toBe(true);
    });
  });

  it('does not call the store when the form is invalid', async () => {
    fixture.componentInstance.form.controls.name.setValue('');
    fixture.nativeElement.querySelector('form')!.dispatchEvent(new Event('submit'));
    await fixture.whenStable();
    expect(store.create).not.toHaveBeenCalled();
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it('submits valid input, calls store.create, and navigates back to the tenant list', async () => {
    store.create.mockReturnValue(of(detail('Acme Corp', 'acme')));

    setInput(fixture, 'input[formControlName="name"]', 'Acme Corp');
    setInput(fixture, 'input[formControlName="slug"]', 'acme');
    setSelect(fixture, 'select[formControlName="plan"]', 'professional');
    setInput(fixture, 'input[formControlName="contactName"]', 'Jane Ops');
    setInput(fixture, 'input[formControlName="contactEmail"]', 'ops@acme.test');

    fixture.nativeElement.querySelector('form')!.dispatchEvent(new Event('submit'));

    await vi.waitFor(() => {
      expect(router.navigate).toHaveBeenCalledWith([
        '/',
        APP_PATHS.platform.base,
        APP_PATHS.platform.tenants,
      ]);
    });
    expect(store.create).toHaveBeenCalledWith({
      name: 'Acme Corp',
      slug: 'acme',
      plan: 'professional',
      contactName: 'Jane Ops',
      contactEmail: 'ops@acme.test',
    });
  });

  it('omits empty contact fields from the payload', async () => {
    store.create.mockReturnValue(of(detail('Acme', 'acme')));

    setInput(fixture, 'input[formControlName="name"]', 'Acme');
    setInput(fixture, 'input[formControlName="slug"]', 'acme');

    fixture.nativeElement.querySelector('form')!.dispatchEvent(new Event('submit'));

    await vi.waitFor(() => {
      expect(store.create).toHaveBeenCalledWith({
        name: 'Acme',
        slug: 'acme',
        plan: 'trial',
        contactName: undefined,
        contactEmail: undefined,
      });
    });
  });

  it('renders the exact backend ErrorDetail.message beside the matching control on a 422 validation_failed', async () => {
    const details: ApiErrorDetail[] = [
      { field: 'slug', code: 'invalid_format', message: 'Slug is already in use' },
    ];
    const error: ApiError = {
      code: 'validation_failed',
      message: 'Validation failed',
      status: 422,
      details,
    };
    store.create.mockReturnValue(throwError(() => error));

    setInput(fixture, 'input[formControlName="name"]', 'Acme');
    setInput(fixture, 'input[formControlName="slug"]', 'acme');
    fixture.nativeElement.querySelector('form')!.dispatchEvent(new Event('submit'));

    await vi.waitFor(() => {
      expect(fixture.componentInstance.form.controls.slug.errors).toEqual({
        server: 'Slug is already in use',
      });
    });
    fixture.detectChanges();

    const slugErrorId = errorIdFor('slug');
    const slugError = fixture.nativeElement.querySelector(`#${slugErrorId}`);
    expect(slugError?.textContent).toContain('Slug is already in use');
    expect(fixture.nativeElement.textContent).toContain('Validation failed');
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it('maps a 409 conflict without details onto the slug control and renders the message beside it', async () => {
    const error: ApiError = {
      code: 'conflict',
      message: 'Tenant slug is already in use',
      status: 409,
    };
    store.create.mockReturnValue(throwError(() => error));

    setInput(fixture, 'input[formControlName="name"]', 'Acme');
    setInput(fixture, 'input[formControlName="slug"]', 'acme');
    fixture.nativeElement.querySelector('form')!.dispatchEvent(new Event('submit'));

    await vi.waitFor(() => {
      expect(fixture.componentInstance.form.controls.slug.errors).toEqual({
        server: 'Tenant slug is already in use',
      });
    });
    fixture.detectChanges();

    expect(fixture.componentInstance.form.controls.slug.touched).toBe(true);
    const slugError = fixture.nativeElement.querySelector(`#${errorIdFor('slug')}`);
    expect(slugError?.textContent).toContain('Tenant slug is already in use');
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it('sets aria-invalid="true" on a control that has a server error', async () => {
    const details: ApiErrorDetail[] = [
      { field: 'slug', code: 'invalid_format', message: 'Slug is already in use' },
    ];
    const error: ApiError = {
      code: 'validation_failed',
      message: 'Validation failed',
      status: 422,
      details,
    };
    store.create.mockReturnValue(throwError(() => error));

    setInput(fixture, 'input[formControlName="name"]', 'Acme');
    setInput(fixture, 'input[formControlName="slug"]', 'acme');
    fixture.nativeElement.querySelector('form')!.dispatchEvent(new Event('submit'));

    await vi.waitFor(() => {
      expect(fixture.componentInstance.form.controls.slug.errors).toEqual({
        server: 'Slug is already in use',
      });
    });
    fixture.detectChanges();

    const slugInput = fixture.nativeElement.querySelector(
      'input[formControlName="slug"]',
    ) as HTMLInputElement;
    expect(slugInput.getAttribute('aria-invalid')).toBe('true');
  });

  it('wires aria-describedby on the slug input to the matching error element', async () => {
    const details: ApiErrorDetail[] = [
      { field: 'slug', code: 'invalid_format', message: 'Slug is already in use' },
    ];
    const error: ApiError = {
      code: 'validation_failed',
      message: 'Validation failed',
      status: 422,
      details,
    };
    store.create.mockReturnValue(throwError(() => error));

    setInput(fixture, 'input[formControlName="name"]', 'Acme');
    setInput(fixture, 'input[formControlName="slug"]', 'acme');
    fixture.nativeElement.querySelector('form')!.dispatchEvent(new Event('submit'));

    await vi.waitFor(() => {
      expect(fixture.componentInstance.form.controls.slug.errors).toEqual({
        server: 'Slug is already in use',
      });
    });
    fixture.detectChanges();

    const slugInput = fixture.nativeElement.querySelector(
      'input[formControlName="slug"]',
    ) as HTMLInputElement;
    const describedBy = slugInput.getAttribute('aria-describedby');
    expect(describedBy).toBe(errorIdFor('slug'));
    const described = fixture.nativeElement.querySelector(`#${describedBy}`);
    expect(described).toBeTruthy();
    expect(described?.textContent).toContain('Slug is already in use');
  });

  it('disables the submit button while submitting and re-enables after', async () => {
    const subject = new Subject<PlatformTenantDetail>();
    store.create.mockReturnValue(subject.asObservable());

    setInput(fixture, 'input[formControlName="name"]', 'Acme');
    setInput(fixture, 'input[formControlName="slug"]', 'acme');
    const button = fixture.nativeElement.querySelector(
      'button[type="submit"]',
    ) as HTMLButtonElement;

    fixture.nativeElement.querySelector('form')!.dispatchEvent(new Event('submit'));
    fixture.detectChanges();
    expect(button.disabled).toBe(true);
    expect(button.textContent).toContain('Creating');

    subject.next(detail('Acme', 'acme'));
    subject.complete();

    await vi.waitFor(
      () => {
        fixture.detectChanges();
        expect(button.disabled).toBe(false);
        expect(button.textContent).toContain('Create tenant');
      },
      { timeout: 2000 },
    );
  });

  it('renders the exact plan server validation message and matching ARIA error element on a 422', async () => {
    const details: ApiErrorDetail[] = [
      {
        field: 'plan',
        code: 'invalid_value',
        message: 'Plan must be one of: trial, starter, professional, enterprise',
      },
    ];
    const error: ApiError = {
      code: 'validation_failed',
      message: 'Validation failed',
      status: 422,
      details,
    };
    store.create.mockReturnValue(throwError(() => error));

    setInput(fixture, 'input[formControlName="name"]', 'Acme');
    setInput(fixture, 'input[formControlName="slug"]', 'acme');
    setSelect(fixture, 'select[formControlName="plan"]', 'professional');
    fixture.nativeElement.querySelector('form')!.dispatchEvent(new Event('submit'));

    await vi.waitFor(() => {
      expect(fixture.componentInstance.form.controls.plan.errors).toEqual({
        server: 'Plan must be one of: trial, starter, professional, enterprise',
      });
    });
    fixture.detectChanges();

    const planErrorId = errorIdFor('plan');
    const planError = fixture.nativeElement.querySelector(`#${planErrorId}`) as HTMLElement;
    expect(planError).toBeTruthy();
    expect(planError.textContent).toContain(
      'Plan must be one of: trial, starter, professional, enterprise',
    );
    expect(fixture.nativeElement.textContent).toContain(
      'Plan must be one of: trial, starter, professional, enterprise',
    );

    const planSelect = fixture.nativeElement.querySelector(
      'select[formControlName="plan"]',
    ) as HTMLSelectElement;
    expect(planSelect.getAttribute('aria-invalid')).toBe('true');
    expect(planSelect.getAttribute('aria-describedby')).toBe(planErrorId);

    expect(router.navigate).not.toHaveBeenCalled();
  });
});

describe('TenantFormComponent (edit mode)', () => {
  let editFixture: ComponentFixture<TenantFormComponent>;
  let editStore: { create: ReturnType<typeof vi.fn>; update: ReturnType<typeof vi.fn> };
  let editService: { get: ReturnType<typeof vi.fn> };
  let editPermissions: { has: ReturnType<typeof vi.fn> };
  let editRouter: Router;

  beforeEach(async () => {
    TestBed.resetTestingModule();
    editStore = { create: vi.fn(), update: vi.fn() };
    editService = {
      get: vi
        .fn()
        .mockReturnValue(
          of({ data: editModeDetail() } satisfies ApiResponse<PlatformTenantDetail>),
        ),
    };
    editPermissions = { has: vi.fn().mockReturnValue(true) };

    await TestBed.configureTestingModule({
      imports: [TenantFormComponent],
      providers: [
        provideRouter([]),
        { provide: TenantsStore, useValue: editStore },
        { provide: PlatformTenantsService, useValue: editService },
        { provide: PermissionsService, useValue: editPermissions },
        {
          provide: ActivatedRoute,
          useValue: {
            snapshot: {
              paramMap: { get: (key: string) => (key === 'id' ? 't-1' : null) },
            },
          },
        },
      ],
    }).compileComponents();

    editRouter = TestBed.inject(Router);
    vi.spyOn(editRouter, 'navigate').mockResolvedValue(true);
    editFixture = TestBed.createComponent(TenantFormComponent);
    editFixture.detectChanges();
  });

  it('fetches the tenant using the :id route param on init', () => {
    expect(editService.get).toHaveBeenCalledWith('t-1');
  });

  it('pre-fills the form with the fetched tenant', () => {
    const form = editFixture.componentInstance.form;
    expect(form.controls.name.value).toBe('Acme Corp');
    expect(form.controls.slug.value).toBe('acme');
    expect(form.controls.plan.value).toBe('professional');
    expect(form.controls.contactName.value).toBe('Jane Ops');
    expect(form.controls.contactEmail.value).toBe('ops@acme.test');
  });

  it('renders the Edit tenant header and Save changes submit label', () => {
    const element: HTMLElement = editFixture.nativeElement;
    expect(element.querySelector('h1')?.textContent).toContain('Edit tenant');
    const button = element.querySelector('button[type="submit"]') as HTMLButtonElement;
    expect(button.textContent).toContain('Save changes');
  });

  it('points the cancel link to the detail page', () => {
    const cancel = editFixture.nativeElement.querySelector('.actions a') as HTMLAnchorElement;
    expect(cancel).toBeTruthy();
    expect(cancel.getAttribute('href')).toBe('/platform/tenants/t-1');
  });

  it('submits via store.update and navigates to the detail page on success', async () => {
    editStore.update.mockReturnValue(of(editModeDetail()));

    setInput(editFixture, 'input[formControlName="name"]', 'Acme Inc');
    editFixture.nativeElement.querySelector('form')!.dispatchEvent(new Event('submit'));

    await vi.waitFor(() => {
      expect(editRouter.navigate).toHaveBeenCalledWith([
        '/',
        APP_PATHS.platform.base,
        APP_PATHS.platform.tenants,
        't-1',
      ]);
    });
    expect(editStore.update).toHaveBeenCalledWith('t-1', {
      name: 'Acme Inc',
      slug: 'acme',
      plan: 'professional',
      contactName: 'Jane Ops',
      contactEmail: 'ops@acme.test',
    });
    expect(editStore.create).not.toHaveBeenCalled();
  });

  it('maps empty contact fields to null in the update payload (clearing behavior)', async () => {
    editStore.update.mockReturnValue(
      of({
        ...editModeDetail(),
        contactName: null,
        contactEmail: null,
      }),
    );

    setInput(editFixture, 'input[formControlName="contactName"]', '');
    setInput(editFixture, 'input[formControlName="contactEmail"]', '');
    editFixture.nativeElement.querySelector('form')!.dispatchEvent(new Event('submit'));

    await vi.waitFor(() => {
      expect(editStore.update).toHaveBeenCalledWith('t-1', {
        name: 'Acme Corp',
        slug: 'acme',
        plan: 'professional',
        contactName: null,
        contactEmail: null,
      });
    });
  });

  it('renders a non-submittable empty state with a Try again button when route fetch fails', () => {
    TestBed.resetTestingModule();
    const error: ApiError = {
      code: 'not_found',
      message: 'Tenant not found',
      status: 404,
    };
    const failingService = {
      get: vi.fn().mockReturnValue(throwError(() => error)),
    };
    const pendingStore = { create: vi.fn(), update: vi.fn() };

    TestBed.configureTestingModule({
      imports: [TenantFormComponent],
      providers: [
        provideRouter([]),
        { provide: TenantsStore, useValue: pendingStore },
        { provide: PlatformTenantsService, useValue: failingService },
        { provide: PermissionsService, useValue: { has: vi.fn().mockReturnValue(true) } },
        {
          provide: ActivatedRoute,
          useValue: {
            snapshot: {
              paramMap: { get: (key: string) => (key === 'id' ? 't-1' : null) },
            },
          },
        },
      ],
    });
    editFixture = TestBed.createComponent(TenantFormComponent);
    editFixture.detectChanges();
    editFixture.detectChanges();

    const empty = editFixture.nativeElement.querySelector('app-empty-state') as HTMLElement;
    expect(empty).toBeTruthy();
    expect(empty.textContent?.toLowerCase()).toContain("couldn't load");

    expect(editFixture.nativeElement.querySelector('form')).toBeNull();

    const retry = empty.querySelector('button') as HTMLButtonElement;
    expect(retry).toBeTruthy();
    expect(retry.textContent).toContain('Try again');

    const callCountBefore = failingService.get.mock.calls.length;
    retry.click();
    editFixture.detectChanges();

    expect(failingService.get.mock.calls.length).toBe(callCountBefore + 1);
  });
});

describe('TenantFormComponent (initial input)', () => {
  let initialFixture: ComponentFixture<TenantFormComponent>;
  let initialStore: { create: ReturnType<typeof vi.fn>; update: ReturnType<typeof vi.fn> };
  let initialService: { get: ReturnType<typeof vi.fn> };
  let initialPermissions: { has: ReturnType<typeof vi.fn> };
  let initialRouter: Router;

  beforeEach(async () => {
    TestBed.resetTestingModule();
    initialStore = { create: vi.fn(), update: vi.fn() };
    initialService = { get: vi.fn() };
    initialPermissions = { has: vi.fn().mockReturnValue(true) };

    await TestBed.configureTestingModule({
      imports: [TenantFormComponent],
      providers: [
        provideRouter([]),
        { provide: TenantsStore, useValue: initialStore },
        { provide: PlatformTenantsService, useValue: initialService },
        { provide: PermissionsService, useValue: initialPermissions },
        {
          provide: ActivatedRoute,
          useValue: {
            snapshot: {
              paramMap: { get: (key: string) => (key === 'id' ? 't-1' : null) },
            },
          },
        },
      ],
    }).compileComponents();

    initialRouter = TestBed.inject(Router);
    vi.spyOn(initialRouter, 'navigate').mockResolvedValue(true);
    initialFixture = TestBed.createComponent(TenantFormComponent);
  });

  it('pre-fills the form from the initial input and does not call the service', () => {
    initialFixture.componentRef.setInput('initial', editModeDetail());
    initialFixture.detectChanges();

    const form = initialFixture.componentInstance.form;
    expect(form.controls.name.value).toBe('Acme Corp');
    expect(form.controls.slug.value).toBe('acme');
    expect(form.controls.plan.value).toBe('professional');
    expect(form.controls.contactName.value).toBe('Jane Ops');
    expect(form.controls.contactEmail.value).toBe('ops@acme.test');

    expect(initialService.get).not.toHaveBeenCalled();
    expect(initialFixture.nativeElement.querySelector('form')).toBeTruthy();
  });
});

describe('TenantFormComponent (initial input only, no route param)', () => {
  let initialOnlyFixture: ComponentFixture<TenantFormComponent>;
  let initialOnlyStore: { create: ReturnType<typeof vi.fn>; update: ReturnType<typeof vi.fn> };
  let initialOnlyService: { get: ReturnType<typeof vi.fn> };
  let initialOnlyPermissions: { has: ReturnType<typeof vi.fn> };
  let initialOnlyRouter: Router;

  beforeEach(async () => {
    TestBed.resetTestingModule();
    initialOnlyStore = { create: vi.fn(), update: vi.fn() };
    initialOnlyService = { get: vi.fn() };
    initialOnlyPermissions = { has: vi.fn().mockReturnValue(true) };

    await TestBed.configureTestingModule({
      imports: [TenantFormComponent],
      providers: [
        provideRouter([]),
        { provide: TenantsStore, useValue: initialOnlyStore },
        { provide: PlatformTenantsService, useValue: initialOnlyService },
        { provide: PermissionsService, useValue: initialOnlyPermissions },
        {
          provide: ActivatedRoute,
          useValue: {
            snapshot: {
              paramMap: { get: () => null },
            },
          },
        },
      ],
    }).compileComponents();

    initialOnlyRouter = TestBed.inject(Router);
    vi.spyOn(initialOnlyRouter, 'navigate').mockResolvedValue(true);
    initialOnlyFixture = TestBed.createComponent(TenantFormComponent);
  });

  it('derives edit mode and the update identity from the initial input when no route param exists', async () => {
    initialOnlyFixture.componentRef.setInput('initial', editModeDetail());
    initialOnlyFixture.detectChanges();

    const element: HTMLElement = initialOnlyFixture.nativeElement;

    expect(element.querySelector('h1')?.textContent).toContain('Edit tenant');
    const button = element.querySelector('button[type="submit"]') as HTMLButtonElement;
    expect(button.textContent).toContain('Save changes');

    const form = initialOnlyFixture.componentInstance.form;
    expect(form.controls.name.value).toBe('Acme Corp');
    expect(form.controls.slug.value).toBe('acme');
    expect(form.controls.plan.value).toBe('professional');
    expect(form.controls.contactName.value).toBe('Jane Ops');
    expect(form.controls.contactEmail.value).toBe('ops@acme.test');

    expect(initialOnlyService.get).not.toHaveBeenCalled();

    initialOnlyStore.update.mockReturnValue(of(editModeDetail()));
    element.querySelector('form')!.dispatchEvent(new Event('submit'));

    await vi.waitFor(() => {
      expect(initialOnlyStore.update).toHaveBeenCalledWith('t-1', {
        name: 'Acme Corp',
        slug: 'acme',
        plan: 'professional',
        contactName: 'Jane Ops',
        contactEmail: 'ops@acme.test',
      });
    });
    expect(initialOnlyStore.create).not.toHaveBeenCalled();
    expect(initialOnlyRouter.navigate).toHaveBeenCalledWith([
      '/',
      APP_PATHS.platform.base,
      APP_PATHS.platform.tenants,
      't-1',
    ]);
  });
});

describe('TenantFormComponent (manage-permission form gate)', () => {
  interface GateHarness {
    fixture: ComponentFixture<TenantFormComponent>;
    permissions: { has: ReturnType<typeof vi.fn> };
    store: { create: ReturnType<typeof vi.fn>; update: ReturnType<typeof vi.fn> };
    service: { get: ReturnType<typeof vi.fn> };
  }

  async function setupHarness(opts: {
    canManage: boolean;
    routeId?: string | null;
  }): Promise<GateHarness> {
    TestBed.resetTestingModule();
    const permissions = { has: vi.fn().mockReturnValue(opts.canManage) };
    const store = { create: vi.fn(), update: vi.fn() };
    const service = { get: vi.fn() };
    const routeId = opts.routeId ?? null;

    await TestBed.configureTestingModule({
      imports: [TenantFormComponent],
      providers: [
        provideRouter([]),
        { provide: TenantsStore, useValue: store },
        { provide: PlatformTenantsService, useValue: service },
        { provide: PermissionsService, useValue: permissions },
        {
          provide: ActivatedRoute,
          useValue: {
            snapshot: {
              paramMap: { get: (key: string) => (key === 'id' ? routeId : null) },
            },
          },
        },
      ],
    }).compileComponents();

    const router = TestBed.inject(Router);
    vi.spyOn(router, 'navigate').mockResolvedValue(true);
    const fixture = TestBed.createComponent(TenantFormComponent);
    fixture.detectChanges();
    return { fixture, permissions, store, service };
  }

  it('renders the form when the user has platform.tenants.manage (new tenant deep link)', async () => {
    const { fixture, permissions } = await setupHarness({
      canManage: true,
      routeId: null,
    });

    expect(permissions.has).toHaveBeenCalledWith('platform.tenants.manage');
    expect(fixture.nativeElement.querySelector('form')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('input[formControlName="name"]')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('app-empty-state')).toBeNull();
  });

  it('hides the form and shows the access-denied empty state on a new-tenant deep link without manage permission', async () => {
    const { fixture, permissions, store, service } = await setupHarness({
      canManage: false,
      routeId: null,
    });

    expect(permissions.has).toHaveBeenCalledWith('platform.tenants.manage');

    const form = fixture.nativeElement.querySelector('form');
    expect(form).toBeNull();
    expect(fixture.nativeElement.querySelector('input[formControlName="name"]')).toBeNull();

    const empty = fixture.nativeElement.querySelector('app-empty-state') as HTMLElement;
    expect(empty).toBeTruthy();
    const heading = empty.querySelector('h2')?.textContent ?? '';
    expect(heading.toLowerCase()).toContain('management not permitted');

    const description = empty.querySelector('p')?.textContent ?? '';
    expect(description.toLowerCase()).toContain('do not have permission to manage tenants');

    expect(fixture.nativeElement.querySelector('button[type="submit"]')).toBeNull();
    expect(store.create).not.toHaveBeenCalled();
    expect(service.get).not.toHaveBeenCalled();
  });

  it('hides the form and shows the access-denied empty state on an edit deep link without manage permission (no service fetch)', async () => {
    const { fixture, permissions, service } = await setupHarness({
      canManage: false,
      routeId: 't-1',
    });

    expect(permissions.has).toHaveBeenCalledWith('platform.tenants.manage');

    expect(fixture.nativeElement.querySelector('form')).toBeNull();
    expect(fixture.nativeElement.querySelector('input[formControlName="name"]')).toBeNull();

    const empty = fixture.nativeElement.querySelector('app-empty-state') as HTMLElement;
    expect(empty).toBeTruthy();
    expect(empty.querySelector('h2')?.textContent?.toLowerCase()).toContain(
      'management not permitted',
    );

    expect(service.get).not.toHaveBeenCalled();
    const retry = empty.querySelector('button') as HTMLButtonElement | null;
    expect(retry).toBeNull();
  });

  it('does not attempt to render the form or submit when access is denied (no store calls, no service calls)', async () => {
    const { fixture, store, service } = await setupHarness({
      canManage: false,
      routeId: null,
    });

    const form = fixture.nativeElement.querySelector('form');
    expect(form).toBeNull();
    expect(fixture.nativeElement.querySelector('button[type="submit"]')).toBeNull();
    expect(store.create).not.toHaveBeenCalled();
    expect(store.update).not.toHaveBeenCalled();
    expect(service.get).not.toHaveBeenCalled();
  });
});

const setInput = (
  fixture: ComponentFixture<TenantFormComponent>,
  selector: string,
  value: string,
): void => {
  const input = fixture.nativeElement.querySelector(selector) as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event('input'));
  fixture.detectChanges();
};

const setSelect = (
  fixture: ComponentFixture<TenantFormComponent>,
  selector: string,
  value: string,
): void => {
  const select = fixture.nativeElement.querySelector(selector) as HTMLSelectElement;
  select.value = value;
  select.dispatchEvent(new Event('change'));
  fixture.detectChanges();
};

const errorIdFor = (field: string): string => `tenant-form-${field}-error`;
