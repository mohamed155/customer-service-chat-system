import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, Subject } from 'rxjs';
import { Permission } from '../../../core/authz/permissions';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { PlatformPlan, PlatformSubscriptionRow } from '../../../core/api/tenant-api.models';
import { PlatformBillingApiService } from './platform-billing-api.service';
import { PlatformBillingComponent } from './platform-billing.component';

const MOCK_PLANS: PlatformPlan[] = [
  {
    id: 'plan-1',
    name: 'Starter',
    monthlyPriceCents: 9900,
    tokenAllowance: 10000,
    conversationAllowance: 100,
    isDefault: true,
    status: 'active',
  },
  {
    id: 'plan-2',
    name: 'Professional',
    monthlyPriceCents: 29900,
    tokenAllowance: 100000,
    conversationAllowance: 500,
    isDefault: false,
    status: 'active',
  },
];

const MOCK_SUBSCRIPTIONS: PlatformSubscriptionRow[] = [
  {
    tenantId: 't-1',
    tenantName: 'Acme Corp',
    planId: 'plan-1',
    planName: 'Starter',
    planIsDefault: true,
    status: 'active',
    explicit: true,
  },
];

function setupComponent() {
  const getPlans = vi.fn().mockReturnValue(of({ data: MOCK_PLANS }));
  const createPlan = vi.fn().mockReturnValue(of({ data: MOCK_PLANS[0] }));
  const updatePlanStatus = vi.fn();
  const getSubscriptions = vi.fn().mockReturnValue(of({ data: MOCK_SUBSCRIPTIONS }));
  const putSubscription = vi.fn();

  TestBed.configureTestingModule({
    imports: [PlatformBillingComponent],
    providers: [
      provideTaiga(),
      provideZonelessChangeDetection(),
      {
        provide: PlatformBillingApiService,
        useValue: { getPlans, createPlan, updatePlanStatus, getSubscriptions, putSubscription },
      },
    ],
  });

  return { getPlans, getSubscriptions };
}

async function renderComponent(permissions: Permission[]): Promise<HTMLElement> {
  const permissionsMock = {
    has: vi.fn((p: Permission) => permissions.includes(p)),
    effective: vi.fn(() => new Set(permissions)),
  };
  TestBed.overrideProvider(PermissionsService, { useValue: permissionsMock });

  await TestBed.compileComponents();
  const fixture = TestBed.createComponent(PlatformBillingComponent);
  fixture.detectChanges();
  await vi.waitFor(() => {
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
  });
  return fixture.nativeElement as HTMLElement;
}

describe('PlatformBillingComponent', () => {
  it('renders plan catalog and tenant subscriptions', async () => {
    setupComponent();
    const element = await renderComponent(['platform.billing.view', 'platform.billing.manage']);
    expect(element.textContent).toContain('Plan Catalog');
    expect(element.textContent).toContain('Tenant Subscriptions');
    expect(element.textContent).toContain('Starter');
    expect(element.textContent).toContain('Professional');
    expect(element.textContent).toContain('$99.00/mo');
    expect(element.textContent).toContain('$299.00/mo');
    expect(element.textContent).toContain('Acme Corp');
  });

  it('shows loading state then content', async () => {
    const getPlans = vi.fn();
    const createPlan = vi.fn();
    const updatePlanStatus = vi.fn();
    const subsSubject = new Subject<{ data: PlatformSubscriptionRow[] }>();
    const getSubscriptions = vi.fn().mockReturnValue(subsSubject);
    const putSubscription = vi.fn();
    const plansSubject = new Subject<{ data: PlatformPlan[] }>();
    getPlans.mockReturnValue(plansSubject);

    const permissionsMock = {
      has: vi.fn().mockReturnValue(true),
      effective: vi.fn(() => new Set(['platform.billing.view' as Permission])),
    };

    TestBed.configureTestingModule({
      imports: [PlatformBillingComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        {
          provide: PlatformBillingApiService,
          useValue: { getPlans, createPlan, updatePlanStatus, getSubscriptions, putSubscription },
        },
        { provide: PermissionsService, useValue: permissionsMock },
      ],
    });

    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(PlatformBillingComponent);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();

    plansSubject.next({ data: MOCK_PLANS });
    subsSubject.next({ data: MOCK_SUBSCRIPTIONS });
    subsSubject.complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
    });
  });

  describe('Finance / Super Admin (view + manage)', () => {
    it('shows create form and retire buttons', async () => {
      setupComponent();
      const element = await renderComponent(['platform.billing.view', 'platform.billing.manage']);
      const buttons = element.querySelectorAll('app-button');
      const buttonTexts = Array.from(buttons).map((b) => b.textContent?.trim());
      expect(buttonTexts.some((t) => t === 'Create' || t === 'Saving…')).toBe(true);
      expect(buttonTexts.some((t) => t === 'Retire')).toBe(true);
    });
  });

  describe('Scenario 6: View-only user (view without manage)', () => {
    it('renders tables but no create form, retire/activate buttons, or assignment controls', async () => {
      setupComponent();
      const element = await renderComponent(['platform.billing.view']);
      expect(element.textContent).toContain('Plan Catalog');
      expect(element.textContent).toContain('Tenant Subscriptions');
      expect(element.textContent).toContain('Starter');
      expect(element.textContent).toContain('Acme Corp');

      // No create form inputs
      expect(element.querySelectorAll('input[placeholder="Name"]').length).toBe(0);
      // No retire buttons (in Actions column)
      const allButtons = element.querySelectorAll('app-button');
      const buttonTexts = Array.from(allButtons).map((b) => b.textContent?.trim());
      expect(buttonTexts.some((t) => t === 'Retire')).toBe(false);
      expect(buttonTexts.some((t) => t === 'Activate')).toBe(false);
      expect(buttonTexts.some((t) => t === 'Create' || t === 'Saving…')).toBe(false);
      // No plan select in subscriptions
      expect(element.querySelectorAll('select').length).toBe(0);
    });
  });
});
