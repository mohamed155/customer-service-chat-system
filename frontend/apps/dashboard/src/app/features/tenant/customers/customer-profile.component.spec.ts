import { provideZonelessChangeDetection, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { APP_CONFIG } from '../../../core/config/app-config';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { TenantContextService } from '../../../core/tenant/tenant-context.service';
import { ApiError } from '../../../core/api/api.models';
import { ConversationSummary, CustomerDetail } from '../../../core/api/tenant-api.models';
import { CustomerProfileComponent } from './customer-profile.component';
import { CustomerProfileStore } from './customer-profile.store';
import { CustomersApiService } from './customers-api.service';

const buildCustomer = (overrides: Partial<CustomerDetail> = {}): CustomerDetail => ({
  id: 'customer-1',
  displayName: 'Sara Ali',
  email: 'sara@example.com',
  phone: '+201001234567',
  channels: ['email', 'phone'],
  identifiers: [
    { id: 'id-1', channel: 'email', identifier: 'sara@example.com' },
    { id: 'id-2', channel: 'whatsapp', identifier: '+201001234567' },
  ],
  metadata: { plan: 'enterprise', region: 'EMEA' },
  createdAt: '2026-07-13T10:00:00Z',
  updatedAt: '2026-07-13T11:00:00Z',
  ...overrides,
});

const buildConversation = (overrides: Partial<ConversationSummary> = {}): ConversationSummary => ({
  id: 'conv-1',
  channel: 'web_chat',
  status: 'open',
  lastActivityAt: '2026-07-13T11:30:00Z',
  createdAt: '2026-07-13T11:25:00Z',
  ...overrides,
});

describe('CustomerProfileComponent', () => {
  const customer = signal<CustomerDetail | null>(null);
  const conversations = signal<readonly ConversationSummary[]>([]);
  const hasMoreConversations = signal(false);
  const loading = signal(false);
  const error = signal<ApiError | null>(null);
  const notFound = signal(false);
  const customerId = signal<string | null>(null);
  const loadProfile = vi.fn();
  const retry = vi.fn();
  const reset = vi.fn();

  const store = {
    customer,
    conversations,
    hasMoreConversations,
    loading,
    error,
    notFound,
    customerId,
    loadProfile,
    retry,
    reset,
  };

  const mockPermissions = { has: vi.fn().mockReturnValue(true) };
  const mockApi = { updateCustomer: vi.fn() };

  function setup(options: { id?: string } = {}) {
    loadProfile.mockReset();
    retry.mockReset();
    reset.mockReset();
    customer.set(null);
    conversations.set([]);
    hasMoreConversations.set(false);
    loading.set(false);
    error.set(null);
    notFound.set(false);
    customerId.set(null);
    mockPermissions.has.mockReset();
    mockPermissions.has.mockReturnValue(true);
    mockApi.updateCustomer.mockReset();

    const paramMap$ = of(convertToParamMap({ id: options.id ?? 'customer-1' }));

    TestBed.configureTestingModule({
      imports: [CustomerProfileComponent],
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
        { provide: PermissionsService, useValue: mockPermissions },
        { provide: CustomersApiService, useValue: mockApi },
        {
          provide: TenantContextService,
          useValue: { activeTenant: vi.fn().mockReturnValue(null) },
        },
        {
          provide: CurrentUserService,
          useValue: {
            currentUser: vi.fn().mockReturnValue({ id: 'current-user', memberships: [] }),
          },
        },
        { provide: CustomerProfileStore, useValue: store },
        {
          provide: ActivatedRoute,
          useValue: {
            paramMap: paramMap$,
            snapshot: { paramMap: convertToParamMap({ id: options.id ?? 'customer-1' }) },
          },
        },
      ],
    });
  }

  it('dispatches store.loadProfile with the route id on init', async () => {
    await setup();
    await TestBed.compileComponents();
    TestBed.createComponent(CustomerProfileComponent);
    expect(loadProfile).toHaveBeenCalledWith('customer-1');
  });

  it('renders the loading state while loading and no customer is loaded', async () => {
    await setup();
    loading.set(true);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomerProfileComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('app-dashboard-card')).toBeNull();
  });

  it('renders contact info, identifiers, metadata, and history when the profile loads', async () => {
    await setup();
    customer.set(buildCustomer());
    conversations.set([
      buildConversation({ id: 'conv-2', status: 'pending', channel: 'whatsapp' }),
      buildConversation({ id: 'conv-1', status: 'open', channel: 'web_chat' }),
    ]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomerProfileComponent);
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Sara Ali');
    expect(text).toContain('sara@example.com');
    expect(text).toContain('+201001234567');
    expect(text).toContain('plan');
    expect(text).toContain('enterprise');
    expect(text).toContain('region');
    expect(text).toContain('EMEA');
    expect(text).toContain('Created');
    expect(text).toContain('Updated');

    expect(
      fixture.nativeElement.querySelectorAll('app-channel-badge').length,
    ).toBeGreaterThanOrEqual(2);
    expect(fixture.nativeElement.querySelectorAll('app-status-badge')).toHaveLength(2);
  });

  it('hides the email/phone rows when the customer has no contact details', async () => {
    await setup();
    customer.set(
      buildCustomer({
        email: null,
        phone: null,
        identifiers: [{ id: 'id-9', channel: 'telegram', identifier: '@sara_t' }],
      }),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomerProfileComponent);
    fixture.detectChanges();

    const cards = Array.from(
      fixture.nativeElement.querySelectorAll('app-dashboard-card') as NodeListOf<HTMLElement>,
    );
    const contactCard = cards.find((card) => card.textContent?.includes('Contact'));
    expect(contactCard).toBeDefined();
    expect(contactCard?.textContent).not.toContain('Email');
    expect(contactCard?.textContent).not.toContain('Phone');
    expect(fixture.nativeElement.textContent).toContain('@sara_t');
  });

  it('renders the empty state when the conversation history is empty', async () => {
    await setup();
    customer.set(buildCustomer());
    conversations.set([]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomerProfileComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('No conversations yet');
  });

  it('renders an error state with a retry action that re-invokes the store', async () => {
    await setup();
    error.set({ code: 'service_unavailable', message: 'Service unavailable', status: 503 });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomerProfileComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Service unavailable');
    const retryButton = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((button) => button.textContent?.trim() === 'Try again');
    expect(retryButton).toBeTruthy();
    retryButton?.click();
    expect(retry).toHaveBeenCalledOnce();
  });

  it('exposes a back link to the customers list using APP_PATHS', async () => {
    await setup();
    customer.set(buildCustomer());
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomerProfileComponent);
    fixture.detectChanges();

    const backLink = fixture.nativeElement.querySelector('a.back-link') as HTMLAnchorElement | null;
    expect(backLink).not.toBeNull();
    expect(backLink?.getAttribute('href')).toBe('/tenant/customers');
  });

  it('shows "No channel identifiers" when identifiers are empty', async () => {
    await setup();
    customer.set(
      buildCustomer({
        identifiers: [],
        metadata: {},
      }),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomerProfileComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('No channel identifiers');
  });

  it('shows "No metadata" when metadata is empty', async () => {
    await setup();
    customer.set(
      buildCustomer({
        metadata: {},
      }),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomerProfileComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('No metadata');
  });

  it('shows all identifier and metadata items when both have values', async () => {
    await setup();
    customer.set(
      buildCustomer({
        identifiers: [
          { id: 'id-1', channel: 'email', identifier: 'sara@example.com' },
          { id: 'id-2', channel: 'whatsapp', identifier: '+201001234567' },
          { id: 'id-3', channel: 'telegram', identifier: '@sara_t' },
        ],
        metadata: { plan: 'enterprise', region: 'EMEA', tier: 'gold' },
      }),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomerProfileComponent);
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent as string;
    expect(text).not.toContain('No channel identifiers');
    expect(text).not.toContain('No metadata');
    expect(text).toContain('sara@example.com');
    expect(text).toContain('@sara_t');
    expect(text).toContain('enterprise');
    expect(text).toContain('EMEA');
    expect(text).toContain('gold');
  });

  it('shows has_more indicator when there are more conversations', async () => {
    await setup();
    customer.set(buildCustomer());
    conversations.set([buildConversation()]);
    hasMoreConversations.set(true);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomerProfileComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('most recent conversations');
  });

  describe('permissions', () => {
    it('hides the edit button when viewer lacks customers.manage', async () => {
      await setup();
      mockPermissions.has.mockReturnValue(false);
      customer.set(buildCustomer());
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(CustomerProfileComponent);
      fixture.detectChanges();

      const editBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Edit');
      expect(editBtn).toBeUndefined();
    });

    it('shows the edit button when agent has customers.manage', async () => {
      await setup();
      customer.set(buildCustomer());
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(CustomerProfileComponent);
      fixture.detectChanges();

      const editBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Edit');
      expect(editBtn).toBeTruthy();
    });

    it('opens edit dialog when clicking Edit', async () => {
      await setup();
      customer.set(buildCustomer());
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(CustomerProfileComponent);
      fixture.detectChanges();

      const editBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Edit')!;
      editBtn.click();
      fixture.detectChanges();

      expect(fixture.nativeElement.querySelector('app-customer-dialog')).toBeTruthy();
    });
  });

  describe('update submission', () => {
    it('refreshes profile data after successful update', async () => {
      await setup();
      mockApi.updateCustomer.mockReturnValue(of({ data: buildCustomer() }));
      customer.set(buildCustomer());
      customerId.set('customer-1');
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(CustomerProfileComponent);
      fixture.detectChanges();

      const editBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Edit')!;
      editBtn.click();
      fixture.detectChanges();

      const submitBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Save changes')!;
      submitBtn.click();
      fixture.detectChanges();

      expect(mockApi.updateCustomer).toHaveBeenCalledWith('customer-1', expect.any(Object));
      expect(loadProfile).toHaveBeenCalledWith('customer-1');
      expect(fixture.nativeElement.querySelector('app-customer-dialog')).toBeNull();
    });

    it('retains server errors when update fails', async () => {
      await setup();
      mockApi.updateCustomer.mockReturnValue(
        throwError(() => ({
          code: 'validation_failed',
          message: 'Validation failed',
          status: 422,
          details: [{ field: 'displayName', code: 'too_short', message: 'Name is too short' }],
        })),
      );
      customer.set(buildCustomer());
      customerId.set('customer-1');
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(CustomerProfileComponent);
      fixture.detectChanges();

      const editBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Edit')!;
      editBtn.click();
      fixture.detectChanges();

      const submitBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Save changes')!;
      submitBtn.click();
      fixture.detectChanges();

      expect(fixture.nativeElement.textContent).toContain('Name is too short');
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      expect((fixture.componentInstance as any).dialogSubmitting()).toBe(false);
    });
  });
});
