import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { IntegrationDetail, IntegrationEventList } from '../../../core/api/tenant-api.models';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { provideTenantStore } from '../../../core/state/testing/tenant-store.testing';
import { IntegrationDetailComponent } from './integration-detail.component';
import { IntegrationsApiService } from './integrations-api.service';

const REQUEST_ID = 'req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f';

const CONNECTED: IntegrationDetail = {
  slug: 'generic-webhook',
  name: 'Generic Webhook',
  description: 'Forward events to a configurable HTTPS endpoint.',
  category: 'automation',
  isAvailable: true,
  status: 'connected',
  configSchema: [
    { key: 'url', label: 'Endpoint URL', kind: 'text', required: true },
    { key: 'signing_secret', label: 'Signing Secret', kind: 'secret', required: true },
  ],
  connection: {
    config: { url: 'https://example.com/hook' },
    secrets: [{ fieldKey: 'signing_secret', hint: 'XYZ' }],
    webhookUrl: 'https://api.example.com/hooks/v1/abc',
    connectedAt: '2026-07-22T10:00:00Z',
    disconnectedAt: null,
  },
};

const EVENTS: IntegrationEventList = {
  data: [
    {
      id: 'e1',
      eventType: 'delivery_rejected',
      outcome: 'failure',
      reason: 'rate_limited',
      actorMembershipId: null,
      createdAt: new Date(Date.now() - 5 * 60_000).toISOString(),
    },
    {
      id: 'e2',
      eventType: 'something_new',
      outcome: 'unknown_outcome',
      reason: 'a_reason_nobody_named',
      actorMembershipId: null,
      createdAt: '2026-07-22T10:00:00Z',
    },
  ],
  pagination: { nextCursor: null, hasMore: false },
};

describe('IntegrationDetailComponent', () => {
  const detail = vi.fn();
  const events = vi.fn();
  const disconnect = vi.fn();

  async function render() {
    TestBed.configureTestingModule({
      imports: [IntegrationDetailComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideRouter([]),
        provideTenantStore(),
        { provide: IntegrationsApiService, useValue: { detail, events, disconnect } },
        { provide: PermissionsService, useValue: { has: () => true } },
      ],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(IntegrationDetailComponent);
    fixture.componentRef.setInput('slug', 'generic-webhook');
    fixture.detectChanges();
    return fixture;
  }

  beforeEach(() => {
    detail.mockReset().mockReturnValue(of({ data: CONNECTED }));
    events.mockReset().mockReturnValue(of({ data: EVENTS }));
    disconnect.mockReset();
  });

  it('names events, outcomes and reasons from the catalog and shows an unknown one as sent', async () => {
    const fixture = await render();
    const el = fixture.nativeElement as HTMLElement;
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(el.querySelectorAll('.event').length).toBe(2);
    });
    const [known, unknown] = Array.from(el.querySelectorAll('.event')).map((e) => e.textContent);
    expect(known).toContain('Delivery rejected');
    expect(known).toContain('Failure');
    expect(known).toContain('Reason: Too many messages at once');
    expect(known).toContain('5m ago');
    expect(unknown).toContain('something_new');
    expect(unknown).toContain('unknown_outcome');
    expect(unknown).toContain('Reason: a_reason_nobody_named');
  });

  it('writes the connection date for the language and names the status', async () => {
    const fixture = await render();
    const el = fixture.nativeElement as HTMLElement;
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(el.textContent).toContain('Connected at');
    });
    expect(el.textContent).toMatch(/Jul 22, 2026/);
    expect(el.querySelector('.summary')?.textContent).toContain('Connected');
    expect(el.textContent).toContain('Stored: ••••XYZ');
  });

  it('words a failed action by its code, never the API text, with the reference', async () => {
    const fixture = await render();
    const el = fixture.nativeElement as HTMLElement;
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(el.textContent).toContain('Disconnect');
    });
    disconnect.mockReturnValue(
      throwError(() => ({
        code: 'integration_not_connected',
        message: 'Integration is not actively connected',
        status: 409,
        requestId: REQUEST_ID,
      })),
    );
    (
      Array.from(el.querySelectorAll('button')).find(
        (b) => b.textContent?.trim() === 'Disconnect',
      ) as HTMLButtonElement
    ).click();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const text = el.textContent as string;
      expect(text).toContain('This integration is not connected.');
      expect(text).not.toContain('Integration is not actively connected');
      expect(el.querySelector('.form-error [data-testid="reference"]')?.textContent).toBe(
        '3c4d5e6f',
      );
    });
  });

  it('says some settings are not valid for a refused form, not the API text', async () => {
    detail.mockReturnValue(
      of({ data: { ...CONNECTED, status: 'not_connected', connection: null } }),
    );
    const connect = vi.fn().mockReturnValue(
      throwError(() => ({
        code: 'validation_failed',
        message: 'Validation failed',
        status: 422,
        requestId: REQUEST_ID,
      })),
    );
    TestBed.configureTestingModule({
      imports: [IntegrationDetailComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideRouter([]),
        provideTenantStore(),
        { provide: IntegrationsApiService, useValue: { detail, events, connect } },
        { provide: PermissionsService, useValue: { has: () => true } },
      ],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(IntegrationDetailComponent);
    fixture.componentRef.setInput('slug', 'generic-webhook');
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(el.textContent).toContain('Connect');
    });
    (
      Array.from(el.querySelectorAll('button')).find(
        (b) => b.textContent?.trim() === 'Connect',
      ) as HTMLButtonElement
    ).click();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(el.textContent).toContain('Some settings are missing or not valid.');
      expect(el.textContent).not.toContain('Validation failed');
    });
  });
});
