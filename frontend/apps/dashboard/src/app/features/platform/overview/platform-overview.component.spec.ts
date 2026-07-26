import { provideZonelessChangeDetection } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { provideRouter } from '@angular/router';
import { APP_CONFIG } from '../../../core/config/app-config';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { PlatformOverviewComponent } from './platform-overview.component';
import { PlatformOverviewStore } from './platform-overview.store';

describe('PlatformOverviewComponent', () => {
  let fixture: ComponentFixture<PlatformOverviewComponent>;
  let permissions: { has: ReturnType<typeof vi.fn> };
  let storeMock: {
    summary: ReturnType<typeof vi.fn>;
    loading: ReturnType<typeof vi.fn>;
    error: ReturnType<typeof vi.fn>;
    from: ReturnType<typeof vi.fn>;
    to: ReturnType<typeof vi.fn>;
    load: ReturnType<typeof vi.fn>;
    setRange: ReturnType<typeof vi.fn>;
  };

  const populatedSummary = {
    range: { from: '2026-06-26', to: '2026-07-26' },
    tenants: { total: 42, active: 30, suspended: 10, createdInRange: 5 },
    activeConversations: 18,
    aiUsage: {
      totalCalls: 1500,
      successfulCalls: 1400,
      failedCalls: 100,
      inputTokens: 50000,
      outputTokens: 100000,
    },
    generatedAt: '2026-07-26T12:00:00Z',
  };

  const zeroSummary = {
    range: { from: '2026-06-26', to: '2026-07-26' },
    tenants: { total: 0, active: 0, suspended: 0, createdInRange: 0 },
    activeConversations: 0,
    aiUsage: { totalCalls: 0, successfulCalls: 0, failedCalls: 0, inputTokens: 0, outputTokens: 0 },
    generatedAt: '2026-07-26T12:00:00Z',
  };

  async function setup(overrides?: {
    summary?: typeof populatedSummary;
    hasDiagnostics?: boolean;
  }) {
    permissions = { has: vi.fn() };
    storeMock = {
      summary: vi.fn(() => overrides?.summary ?? populatedSummary),
      loading: vi.fn(() => false),
      error: vi.fn(() => null),
      from: vi.fn(() => null),
      to: vi.fn(() => null),
      load: vi.fn(),
      setRange: vi.fn(),
    };

    permissions.has.mockImplementation((perm: string) => {
      if (perm === 'platform.diagnostics.view') {
        return overrides?.hasDiagnostics ?? false;
      }
      return false;
    });

    TestBed.configureTestingModule({
      imports: [PlatformOverviewComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideRouter([]),
        { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1' } },
        { provide: PermissionsService, useValue: permissions },
      ],
    })
      .overrideComponent(PlatformOverviewComponent, {
        set: { providers: [{ provide: PlatformOverviewStore, useValue: storeMock }] },
      })
      .compileComponents();

    fixture = TestBed.createComponent(PlatformOverviewComponent);
    fixture.detectChanges();
  }

  it('renders tenant total and AI sub-texts from a populated summary', async () => {
    await setup();
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('42');
    expect(text).toContain('30 active');
    expect(text).toContain('10 suspended');
    expect(text).toContain('5 created in range');
    expect(text).toContain('1400 success');
    expect(text).toContain('100 failed');
  });

  it('shows the "No data yet" empty state when every figure is zero', async () => {
    await setup({ summary: zeroSummary });
    expect(fixture.nativeElement.textContent).toContain('No data yet');
  });

  it('hides System Health link when permission is denied', async () => {
    await setup({ hasDiagnostics: false });
    expect(fixture.nativeElement.textContent).not.toContain('System Health');
  });

  it('shows System Health link when permission is granted', async () => {
    await setup({ hasDiagnostics: true });
    expect(fixture.nativeElement.textContent).toContain('System Health');
  });
});
