import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, Subject } from 'rxjs';
import { APP_CONFIG } from '../../core/config/app-config';
import { ApiService } from '../../core/api/api.service';
import { NotificationsService } from '../../core/realtime/notifications.service';
import { RealtimeService, SseEvent } from '../../core/realtime/realtime.service';
import { AvailabilityToggleComponent } from './availability-toggle.component';

describe('AvailabilityToggleComponent', () => {
  let api: { get: ReturnType<typeof vi.fn>; put: ReturnType<typeof vi.fn> };
  let realtimeSubject: Subject<SseEvent>;
  let requestPermission: ReturnType<typeof vi.fn>;

  function setup() {
    TestBed.configureTestingModule({
      imports: [AvailabilityToggleComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1' } },
        { provide: ApiService, useValue: api },
        { provide: RealtimeService, useValue: { events: () => realtimeSubject.asObservable() } },
        {
          provide: NotificationsService,
          useValue: { requestPermission },
        },
      ],
    });
  }

  beforeEach(() => {
    api = { get: vi.fn(), put: vi.fn() };
    realtimeSubject = new Subject<SseEvent>();
    requestPermission = vi.fn();
  });

  it('loads state on init', async () => {
    api.get.mockReturnValue(
      of({ data: { membershipId: 'm-1', state: 'available', stateChangedAt: null } }),
    );
    setup();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AvailabilityToggleComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.textContent).toContain('Available');
    });
  });

  it('sends PUT request on click', async () => {
    api.get.mockReturnValue(
      of({ data: { membershipId: 'm-1', state: 'away', stateChangedAt: null } }),
    );
    api.put.mockReturnValue(
      of({
        data: { membershipId: 'm-1', state: 'available', stateChangedAt: '2026-07-14T10:00:00Z' },
      }),
    );
    setup();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AvailabilityToggleComponent);
    fixture.detectChanges();
    await vi.waitFor(() => expect(fixture.nativeElement.textContent).toContain('Away'));

    fixture.nativeElement.querySelector('.toggle').click();

    await vi.waitFor(() => {
      expect(api.put).toHaveBeenCalledWith('tenant/availability/me', { state: 'available' });
    });
  });

  it('reacts to availability.changed SSE events', async () => {
    api.get.mockReturnValue(
      of({ data: { membershipId: 'm-1', state: 'away', stateChangedAt: null } }),
    );
    setup();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AvailabilityToggleComponent);
    fixture.detectChanges();
    await vi.waitFor(() => expect(fixture.nativeElement.textContent).toContain('Away'));

    realtimeSubject.next({
      event: 'availability.changed',
      id: '1',
      data: JSON.stringify({ state: 'available' }),
    });
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Available');
  });

  it('requests Notification permission exactly once on toggle-to-available only', async () => {
    api.get.mockReturnValue(
      of({ data: { membershipId: 'm-1', state: 'away', stateChangedAt: null } }),
    );
    api.put.mockReturnValue(
      of({
        data: { membershipId: 'm-1', state: 'available', stateChangedAt: '2026-07-14T10:00:00Z' },
      }),
    );
    setup();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AvailabilityToggleComponent);
    fixture.detectChanges();
    await vi.waitFor(() => expect(fixture.nativeElement.textContent).toContain('Away'));

    fixture.nativeElement.querySelector('.toggle').click();
    await vi.waitFor(() => expect(requestPermission).toHaveBeenCalledTimes(1));

    api.put.mockReturnValue(
      of({ data: { membershipId: 'm-1', state: 'away', stateChangedAt: '2026-07-14T10:01:00Z' } }),
    );
    fixture.nativeElement.querySelector('.toggle').click();
    await vi.waitFor(() => expect(fixture.nativeElement.textContent).toContain('Away'));

    expect(requestPermission).toHaveBeenCalledTimes(1);
  });
});
