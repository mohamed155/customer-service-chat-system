import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ActivatedRoute, provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { IntegrationList, IntegrationListItem } from '../../../core/api/tenant-api.models';
import { IntegrationsApiService } from './integrations-api.service';
import { IntegrationsComponent } from './integrations.component';
import { IntegrationsStore } from './integrations.store';

const MOCK_ITEMS: IntegrationListItem[] = [
  {
    slug: 'generic-webhook',
    name: 'Generic Webhook',
    description: 'Receive events from any system that can send signed webhooks.',
    category: 'automation',
    isAvailable: true,
    status: 'not_connected',
  },
];

const MOCK_LIST: IntegrationList = { items: MOCK_ITEMS };
const MOCK_RESPONSE: ApiResponse<IntegrationList> = { data: MOCK_LIST };

describe('IntegrationsComponent', () => {
  const list = vi.fn();

  beforeEach(() => {
    list.mockReset();
    TestBed.configureTestingModule({
      imports: [IntegrationsComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideRouter([]),
        IntegrationsStore,
        { provide: IntegrationsApiService, useValue: { list } },
        { provide: ActivatedRoute, useValue: {} },
      ],
    });
  });

  it('moves from loading to content when items load', async () => {
    list.mockReturnValue(of(MOCK_RESPONSE));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(IntegrationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
      const grid = fixture.nativeElement.querySelector('section.grid');
      expect(grid).toBeTruthy();
    });
  });

  it('shows the empty state when there are no items', async () => {
    list.mockReturnValue(of({ data: { items: [] } } as ApiResponse<IntegrationList>));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(IntegrationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
  });

  it('moves from loading to error and retries', async () => {
    list.mockReturnValue(throwError(() => new Error('fail')));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(IntegrationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    });

    list.mockReturnValue(of(MOCK_RESPONSE));
    const retryBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Try again')!;
    retryBtn.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
    });
  });
});
