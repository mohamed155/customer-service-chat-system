import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, Subject, throwError } from 'rxjs';
import { BillingOverview } from '../../../core/api/tenant-api.models';
import { BillingApiService } from './billing-api.service';
import { BillingStore } from './billing.store';
import { BillingPageComponent } from './billing-page.component';

const MOCK_OVERVIEW: BillingOverview = {
  plan: {
    id: 'plan-pro-001',
    name: 'Professional',
    monthlyPriceCents: 29900,
    tokenAllowance: 100_000,
    conversationAllowance: 500,
    isDefault: false,
    status: 'active',
  },
  subscriptionStatus: 'active',
  period: {
    start: new Date('2026-07-01T00:00:00Z'),
    end: new Date('2026-08-01T00:00:00Z'),
  },
  usage: {
    tokens: { used: 40000, allowance: 100_000, percent: 40, state: 'normal' },
    conversations: { used: 100, allowance: 500, percent: 20, state: 'normal' },
  },
  aiPaused: false,
  aiPausedReason: null,
};

const UNLIMITED_OVERVIEW: BillingOverview = {
  plan: {
    id: 'plan-enterprise-003',
    name: 'Enterprise',
    monthlyPriceCents: 99900,
    tokenAllowance: null,
    conversationAllowance: null,
    isDefault: false,
    status: 'active',
  },
  subscriptionStatus: 'active',
  period: {
    start: new Date('2026-07-01T00:00:00Z'),
    end: new Date('2026-08-01T00:00:00Z'),
  },
  usage: {
    tokens: { used: 250000, allowance: null, percent: null, state: 'normal' },
    conversations: { used: 1200, allowance: null, percent: null, state: 'normal' },
  },
  aiPaused: false,
  aiPausedReason: null,
};

const WARNING_OVERVIEW: BillingOverview = {
  plan: {
    id: 'plan-pro-001',
    name: 'Professional',
    monthlyPriceCents: 29900,
    tokenAllowance: 100_000,
    conversationAllowance: 500,
    isDefault: false,
    status: 'active',
  },
  subscriptionStatus: 'active',
  period: {
    start: new Date('2026-07-01T00:00:00Z'),
    end: new Date('2026-08-01T00:00:00Z'),
  },
  usage: {
    tokens: { used: 85000, allowance: 100_000, percent: 85, state: 'warning' },
    conversations: { used: 420, allowance: 500, percent: 84, state: 'warning' },
  },
  aiPaused: false,
  aiPausedReason: null,
};

const EXCEEDED_OVERVIEW: BillingOverview = {
  plan: {
    id: 'plan-starter-002',
    name: 'Starter',
    monthlyPriceCents: 9900,
    tokenAllowance: 50_000,
    conversationAllowance: 200,
    isDefault: false,
    status: 'active',
  },
  subscriptionStatus: 'active',
  period: {
    start: new Date('2026-07-01T00:00:00Z'),
    end: new Date('2026-08-01T00:00:00Z'),
  },
  usage: {
    tokens: { used: 50000, allowance: 50000, percent: 100, state: 'exceeded' },
    conversations: { used: 200, allowance: 200, percent: 100, state: 'exceeded' },
  },
  aiPaused: true,
  aiPausedReason: 'token_limit',
};

const ZERO_USAGE_OVERVIEW: BillingOverview = {
  plan: {
    id: 'plan-pro-001',
    name: 'Professional',
    monthlyPriceCents: 29900,
    tokenAllowance: 100_000,
    conversationAllowance: 500,
    isDefault: false,
    status: 'active',
  },
  subscriptionStatus: 'active',
  period: {
    start: new Date('2026-07-01T00:00:00Z'),
    end: new Date('2026-08-01T00:00:00Z'),
  },
  usage: {
    tokens: { used: 0, allowance: 100_000, percent: 0, state: 'normal' },
    conversations: { used: 0, allowance: 500, percent: 0, state: 'normal' },
  },
  aiPaused: false,
  aiPausedReason: null,
};

function createComponent(): {
  fixture: ReturnType<typeof TestBed.createComponent<BillingPageComponent>>;
  store: ReturnType<typeof fixture.componentRef.injector.get<typeof BillingStore>>;
} {
  const fixture = TestBed.createComponent(BillingPageComponent);
  const store = fixture.componentRef.injector.get(BillingStore);
  return { fixture, store };
}

async function renderWithOverview(overview: BillingOverview): Promise<HTMLElement> {
  const getOverview = TestBed.inject(BillingApiService).getOverview as ReturnType<typeof vi.fn>;
  getOverview.mockReturnValue(of({ data: overview }));
  await TestBed.compileComponents();
  const { fixture, store } = createComponent();
  store.load();
  fixture.detectChanges();
  await vi.waitFor(() => {
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
  });
  return fixture.nativeElement as HTMLElement;
}

describe('BillingPageComponent', () => {
  const getOverview = vi.fn();

  beforeEach(() => {
    getOverview.mockReset();
    getOverview.mockReturnValue(of({ data: MOCK_OVERVIEW }));
    TestBed.configureTestingModule({
      imports: [BillingPageComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: BillingApiService, useValue: { getOverview } },
      ],
    });
  });

  it('renders plan name and price', async () => {
    const element = await renderWithOverview(MOCK_OVERVIEW);
    expect(element.textContent).toContain('Professional');
    expect(element.textContent).toContain('$299.00/mo');
  });

  it('writes the period, the percentage and the numbers for the language', async () => {
    const element = await renderWithOverview(MOCK_OVERVIEW);
    expect(element.querySelector('.period')?.textContent).toBe('Jul 1, 2026 – Aug 1, 2026');
    const notes = Array.from(element.querySelectorAll('.metric-footnote')).map((n) =>
      n.textContent?.trim(),
    );
    expect(notes).toEqual(['40.0% used', '20.0% used']);
    expect(element.querySelector('.plan-price')?.textContent).toBe('$299.00/mo');
  });

  it('names the subscription status from the catalog and shows an unknown one as sent', async () => {
    const pastDue = await renderWithOverview({ ...MOCK_OVERVIEW, subscriptionStatus: 'past_due' });
    expect(pastDue.textContent).toContain('Past due');
    TestBed.resetTestingModule();
  });

  it('words a failed load by its code with the request reference, never the API text', async () => {
    getOverview.mockReturnValue(
      throwError(() => ({
        code: 'internal_error',
        message: 'Failed to load billing',
        status: 500,
        requestId: 'req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f',
      })),
    );
    await TestBed.compileComponents();
    const { fixture, store } = createComponent();
    store.load();
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('The service is temporarily unavailable. Please try again.');
      expect(text).not.toContain('Failed to load billing');
      expect(fixture.nativeElement.querySelector('[data-testid="reference"]')?.textContent).toBe(
        '3c4d5e6f',
      );
    });
  });

  it('renders token and conversation usage cards', async () => {
    const element = await renderWithOverview(MOCK_OVERVIEW);
    expect(element.textContent).toContain('Tokens');
    expect(element.textContent).toContain('Conversations');
    expect(element.textContent).toContain('40,000');
    expect(element.textContent).toContain('100');
    expect(element.textContent).toContain('100,000');
    expect(element.textContent).toContain('500');
  });

  it('shows Unlimited and bar-fill at 0% for unlimited allowance', async () => {
    const element = await renderWithOverview(UNLIMITED_OVERVIEW);
    expect(element.textContent).toContain('Enterprise');
    expect(element.textContent).toContain('$999.00/mo');
    expect(element.textContent).toContain('Unlimited');
    const barFills = element.querySelectorAll('.bar-fill');
    for (const bar of barFills) {
      expect((bar as HTMLElement).style.width).toBe('0%');
    }
  });

  it('renders 0 not empty for zero usage', async () => {
    const element = await renderWithOverview(ZERO_USAGE_OVERVIEW);
    expect(element.textContent).toContain('0');
    expect(element.textContent).toContain('0.0%');
  });

  it('shows loading state while API in flight then overview', async () => {
    const subject = new Subject<{ data: BillingOverview }>();
    getOverview.mockReturnValue(subject);
    await TestBed.compileComponents();
    const { fixture, store } = createComponent();
    store.load();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
    subject.next({ data: MOCK_OVERVIEW });
    subject.complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
    });
  });

  it('renders warning CSS class at 85% usage', async () => {
    const element = await renderWithOverview(WARNING_OVERVIEW);
    const warningCards = element.querySelectorAll('app-dashboard-card.warning');
    expect(warningCards.length).toBe(2);
  });

  it('renders exceeded CSS class at 100% usage', async () => {
    const element = await renderWithOverview(EXCEEDED_OVERVIEW);
    const exceededCards = element.querySelectorAll('app-dashboard-card.exceeded');
    expect(exceededCards.length).toBe(2);
  });

  it('shows AI-paused banner with token_limit copy', async () => {
    const element = await renderWithOverview(EXCEEDED_OVERVIEW);
    const banner = element.querySelector('app-inline-alert[tone="warning"]');
    expect(banner).toBeTruthy();
    expect(banner?.textContent).toContain(
      'AI replies are paused — the monthly token allowance is used up.',
    );
  });

  it('shows AI-paused banner with conversation_limit copy', async () => {
    const paused: BillingOverview = {
      ...MOCK_OVERVIEW,
      aiPaused: true,
      aiPausedReason: 'conversation_limit',
    };
    const element = await renderWithOverview(paused);
    const banner = element.querySelector('app-inline-alert[tone="warning"]');
    expect(banner).toBeTruthy();
    expect(banner?.textContent).toContain(
      'AI replies are paused — the monthly conversation allowance is used up.',
    );
  });

  it('shows AI-paused banner with subscription_status copy', async () => {
    const paused: BillingOverview = {
      ...MOCK_OVERVIEW,
      subscriptionStatus: 'past_due',
      aiPaused: true,
      aiPausedReason: 'subscription_status',
    };
    const element = await renderWithOverview(paused);
    const banner = element.querySelector('app-inline-alert[tone="warning"]');
    expect(banner).toBeTruthy();
    expect(banner?.textContent).toContain(
      'AI replies are paused — the subscription is past due or canceled.',
    );
  });

  it('keeps unlimited metric normal with no banner', async () => {
    const element = await renderWithOverview(UNLIMITED_OVERVIEW);
    const warningCards = element.querySelectorAll('app-dashboard-card.warning');
    const exceededCards = element.querySelectorAll('app-dashboard-card.exceeded');
    expect(warningCards.length).toBe(0);
    expect(exceededCards.length).toBe(0);
    const banner = element.querySelector('app-inline-alert[tone="warning"]');
    expect(banner).toBeFalsy();
  });
});
