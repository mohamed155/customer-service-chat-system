import { provideZonelessChangeDetection } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { APP_CONFIG } from '../../../core/config/app-config';
import { AiProvidersComponent } from './ai-providers.component';
import { AiProvidersStore } from './ai-providers.store';

describe('AiProvidersComponent', () => {
  let fixture: ComponentFixture<AiProvidersComponent>;

  const mockStatus = {
    window: { from: '2026-07-25T12:00:00Z', to: '2026-07-26T12:00:00Z', preset: '24h' },
    providers: [
      {
        provider: 'openai',
        verdict: 'healthy',
        totalCalls: 200,
        successes: 198,
        failures: 2,
        failureRate: 0.01,
        p50LatencyMs: 350,
        p95LatencyMs: 1200,
        inputTokens: 40000,
        outputTokens: 80000,
        failuresByCategory: {
          authentication: 0,
          rateLimited: 1,
          unavailable: 0,
          timeout: 1,
          invalidRequest: 0,
        },
      },
      {
        provider: 'anthropic',
        verdict: 'degraded',
        totalCalls: 100,
        successes: 96,
        failures: 4,
        failureRate: 0.04,
        p50LatencyMs: 400,
        p95LatencyMs: 1500,
        inputTokens: 20000,
        outputTokens: 40000,
        failuresByCategory: {
          authentication: 0,
          rateLimited: 2,
          unavailable: 1,
          timeout: 1,
          invalidRequest: 0,
        },
      },
      {
        provider: 'gemini',
        verdict: 'no_activity',
        totalCalls: 0,
        successes: 0,
        failures: 0,
        failureRate: null,
        p50LatencyMs: null,
        p95LatencyMs: null,
        inputTokens: 0,
        outputTokens: 0,
        failuresByCategory: {
          authentication: 0,
          rateLimited: 0,
          unavailable: 0,
          timeout: 0,
          invalidRequest: 0,
        },
      },
    ],
    recentFailures: [
      {
        provider: 'openai',
        model: 'gpt-4',
        errorCategory: 'rate_limited',
        latencyMs: 2300,
        occurredAt: '2026-07-26T11:00:00Z',
      },
      {
        provider: 'anthropic',
        model: 'claude-3',
        errorCategory: 'timeout',
        latencyMs: 15000,
        occurredAt: '2026-07-26T10:30:00Z',
      },
    ],
    generatedAt: '2026-07-26T12:00:00Z',
  };

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [AiProvidersComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1' } },
      ],
    })
      .overrideComponent(AiProvidersComponent, {
        set: {
          providers: [
            {
              provide: AiProvidersStore,
              useValue: {
                status: vi.fn(() => mockStatus),
                window: vi.fn(() => '24h'),
                loading: vi.fn(() => false),
                error: vi.fn(() => null),
                load: vi.fn(),
              },
            },
          ],
        },
      })
      .compileComponents();

    fixture = TestBed.createComponent(AiProvidersComponent);
    fixture.detectChanges();
  });

  it('renders verdict labels for all three providers and — for no_activity', () => {
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Healthy');
    expect(text).toContain('Degraded');
    expect(text).toContain('No_activity');
    expect(text).toContain('—');
  });

  it('renders the five failure-category labels', () => {
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('rate_limited');
    expect(text).toContain('Unavailable');
    expect(text).toContain('timeout');
    expect(text).toContain('Invalid request');
  });

  it('renders recent-failures rows with model and latency', () => {
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('gpt-4');
    expect(text).toContain('claude-3');
    expect(text).toContain('2300');
    expect(text).toContain('15000');
  });
});
