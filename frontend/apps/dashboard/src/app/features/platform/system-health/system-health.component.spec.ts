import { provideZonelessChangeDetection } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { APP_CONFIG } from '../../../core/config/app-config';
import { SystemHealthComponent } from './system-health.component';
import { SystemHealthStore } from './system-health.store';

interface MockHealth {
  overall: string;
  dependencies: {
    name: string;
    verdict: string;
    probeMs: number;
    reason: string | null;
    detail: Record<string, unknown> | null;
  }[];
  readAt: string;
}

describe('SystemHealthComponent', () => {
  let fixture: ComponentFixture<SystemHealthComponent>;

  const healthyReading: MockHealth = {
    overall: 'healthy',
    dependencies: [
      { name: 'database', verdict: 'healthy', probeMs: 5, detail: null, reason: null },
      { name: 'cache', verdict: 'healthy', probeMs: 3, detail: null, reason: null },
      {
        name: 'queue',
        verdict: 'healthy',
        probeMs: 10,
        reason: null,
        detail: {
          kind: 'queue',
          pendingCount: 0,
          oldestPendingAgeSeconds: null,
          stuckRetryCount: 0,
        },
      },
      {
        name: 'storage',
        verdict: 'healthy',
        probeMs: 20,
        reason: null,
        detail: { kind: 'storage', backend: 'in_memory', s3Configured: false },
      },
    ],
    readAt: '2026-07-26T12:00:00Z',
  };

  const degradedReading: MockHealth = {
    overall: 'degraded',
    dependencies: [
      { name: 'database', verdict: 'healthy', probeMs: 5, detail: null, reason: null },
      { name: 'cache', verdict: 'healthy', probeMs: 3, detail: null, reason: null },
      {
        name: 'queue',
        verdict: 'degraded',
        probeMs: 120,
        reason: '150 events waiting; oldest waiting 12m',
        detail: {
          kind: 'queue',
          pendingCount: 150,
          oldestPendingAgeSeconds: 720,
          stuckRetryCount: 0,
        },
      },
      {
        name: 'storage',
        verdict: 'healthy',
        probeMs: 45,
        reason: null,
        detail: { kind: 'storage', backend: 'in_memory', s3Configured: false },
      },
    ],
    readAt: '2026-07-26T12:00:00Z',
  };

  const unhealthyReading: MockHealth = {
    overall: 'unhealthy',
    dependencies: [
      {
        name: 'database',
        verdict: 'unhealthy',
        probeMs: 5000,
        reason: 'connection refused',
        detail: null,
      },
      { name: 'cache', verdict: 'healthy', probeMs: 3, detail: null, reason: null },
      { name: 'queue', verdict: 'healthy', probeMs: 10, detail: null, reason: null },
      { name: 'storage', verdict: 'healthy', probeMs: 20, detail: null, reason: null },
    ],
    readAt: '2026-07-26T12:00:00Z',
  };

  function configureModule(reading: MockHealth) {
    TestBed.configureTestingModule({
      imports: [SystemHealthComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1' } },
      ],
    }).overrideComponent(SystemHealthComponent, {
      set: {
        providers: [
          {
            provide: SystemHealthStore,
            useValue: {
              health: vi.fn(() => reading),
              loading: vi.fn(() => false),
              error: vi.fn(() => null),
              load: vi.fn(),
            },
          },
        ],
      },
    });
  }

  it('renders verdict labels and probe values for all four dependencies', () => {
    configureModule(healthyReading);
    fixture = TestBed.createComponent(SystemHealthComponent);
    fixture.detectChanges();
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Healthy');
    expect(text).toContain('Probe: 5ms');
    expect(text).toContain('Probe: 3ms');
    expect(text).toContain('Probe: 10ms');
    expect(text).toContain('Probe: 20ms');
    expect(text).toContain('Backend: in_memory');
  });

  it('shows queue pendingCount and oldestPendingAgeSeconds when detail is present', () => {
    configureModule(degradedReading);
    fixture = TestBed.createComponent(SystemHealthComponent);
    fixture.detectChanges();
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('150');
    expect(text).toContain('720');
    expect(text).toContain('Degraded');
  });

  it('renders the unhealthy reason while other cards still render', () => {
    configureModule(unhealthyReading);
    fixture = TestBed.createComponent(SystemHealthComponent);
    fixture.detectChanges();
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('connection refused');
    expect(text).toContain('Unhealthy');
    expect(text).toContain('Probe: 5000ms');
    expect(text).toContain('Probe: 10ms');
    expect(text).toContain('Probe: 20ms');
  });
});
