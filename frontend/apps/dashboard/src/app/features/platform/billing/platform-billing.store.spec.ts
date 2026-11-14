import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { PlatformPlan, PlatformSubscriptionRow } from '../../../core/api/tenant-api.models';
import { PlatformBillingApiService } from './platform-billing-api.service';
import { PlatformBillingStore } from './platform-billing.store';

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
  {
    tenantId: 't-2',
    tenantName: 'Globex Inc',
    planId: 'plan-2',
    planName: 'Professional',
    planIsDefault: false,
    status: 'active',
    explicit: false,
  },
];

function setupApiMocks() {
  const getPlans = vi.fn().mockReturnValue(of({ data: MOCK_PLANS }));
  const createPlan = vi.fn().mockReturnValue(of({ data: MOCK_PLANS[0] }));
  const updatePlanStatus = vi
    .fn()
    .mockReturnValue(of({ data: { ...MOCK_PLANS[0], status: 'retired' } }));
  const getSubscriptions = vi.fn().mockReturnValue(of({ data: MOCK_SUBSCRIPTIONS }));
  const putSubscription = vi
    .fn()
    .mockReturnValue(of({ data: { tenantId: 't-1', planId: 'plan-2', status: 'active' } }));

  TestBed.configureTestingModule({
    providers: [
      PlatformBillingStore,
      {
        provide: PlatformBillingApiService,
        useValue: { getPlans, createPlan, updatePlanStatus, getSubscriptions, putSubscription },
      },
    ],
  });

  return { getPlans, createPlan, updatePlanStatus, getSubscriptions, putSubscription };
}

describe('PlatformBillingStore', () => {
  describe('loadAll', () => {
    it('populates plans and subscriptions', async () => {
      const { getPlans, getSubscriptions } = setupApiMocks();
      const store = TestBed.inject(PlatformBillingStore);
      await vi.waitFor(() => {
        expect(store.plans()).toEqual(MOCK_PLANS);
        expect(store.subscriptions()).toEqual(MOCK_SUBSCRIPTIONS);
        expect(store.loading()).toBe(false);
      });
      expect(getPlans).toHaveBeenCalledTimes(1);
      expect(getSubscriptions).toHaveBeenCalledTimes(1);
    });

    it('sets error when getPlans fails', async () => {
      setupApiMocks();
      const getPlans = TestBed.inject(PlatformBillingApiService).getPlans as ReturnType<
        typeof vi.fn
      >;
      getPlans.mockReturnValue(throwError(() => new Error('API error')));
      const store = TestBed.inject(PlatformBillingStore);
      await vi.waitFor(() => {
        expect(store.error()).toMatchObject({ code: 'unknown_error' });
        expect(store.loading()).toBe(false);
      });
    });
  });

  describe('createPlan', () => {
    it('calls API and appends plan to list', async () => {
      const { createPlan } = setupApiMocks();
      const store = TestBed.inject(PlatformBillingStore);
      await vi.waitFor(() => expect(store.plans().length).toBe(2));

      store.createPlan({
        name: 'Enterprise',
        monthlyPriceCents: 99900,
      });
      await vi.waitFor(() => {
        expect(store.plans().length).toBe(3);
        expect(store.saving()).toBe(false);
      });
      expect(createPlan).toHaveBeenCalledTimes(1);
    });
  });

  describe('retirePlan / activatePlan', () => {
    it('retirePlan calls API and updates plan status', async () => {
      const { updatePlanStatus } = setupApiMocks();
      updatePlanStatus.mockReturnValue(of({ data: { ...MOCK_PLANS[0], status: 'retired' } }));
      const store = TestBed.inject(PlatformBillingStore);
      await vi.waitFor(() => expect(store.plans().length).toBe(2));

      store.retirePlan('plan-1');
      await vi.waitFor(() => {
        const plan = store.plans().find((p) => p.id === 'plan-1');
        expect(plan?.status).toBe('retired');
        expect(store.saving()).toBe(false);
      });
      expect(updatePlanStatus).toHaveBeenCalledWith('plan-1', 'retired');
    });

    it('activatePlan calls API and updates plan status', async () => {
      const { updatePlanStatus } = setupApiMocks();
      updatePlanStatus.mockReturnValue(of({ data: { ...MOCK_PLANS[0], status: 'active' } }));
      const store = TestBed.inject(PlatformBillingStore);
      await vi.waitFor(() => expect(store.plans().length).toBe(2));

      store.activatePlan('plan-1');
      await vi.waitFor(() => {
        const plan = store.plans().find((p) => p.id === 'plan-1');
        expect(plan?.status).toBe('active');
        expect(store.saving()).toBe(false);
      });
      expect(updatePlanStatus).toHaveBeenCalledWith('plan-1', 'active');
    });
  });

  describe('assignSubscription', () => {
    it('calls API and reloads subscriptions', async () => {
      const { putSubscription, getSubscriptions } = setupApiMocks();
      const store = TestBed.inject(PlatformBillingStore);
      await vi.waitFor(() => expect(store.subscriptions().length).toBe(2));

      store.assignSubscription('t-1', 'plan-2', undefined as unknown as string);
      await vi.waitFor(() => {
        expect(store.saving()).toBe(false);
      });
      expect(putSubscription).toHaveBeenCalledWith('t-1', {
        planId: 'plan-2',
        status: undefined,
      });
      expect(getSubscriptions).toHaveBeenCalledTimes(2);
    });
  });
});
