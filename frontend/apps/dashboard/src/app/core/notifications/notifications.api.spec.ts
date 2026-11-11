import { TestBed } from '@angular/core/testing';
import { provideZonelessChangeDetection } from '@angular/core';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { APP_CONFIG } from '../config/app-config';
import { NotificationsApiService } from './notifications.api';

// The notification DTOs are camelCase on the wire (`NotificationDto`), the pagination block is
// snake_case. The dashboard once read `subject_type` and `created_at`, which the backend never
// sends, so every notification came out with undefined fields.
describe('NotificationsApiService wire format', () => {
  let http: HttpTestingController;
  let service: NotificationsApiService;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideZonelessChangeDetection(),
        provideHttpClient(),
        provideHttpClientTesting(),
        { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1' } },
      ],
    });
    http = TestBed.inject(HttpTestingController);
    service = TestBed.inject(NotificationsApiService);
  });

  afterEach(() => http.verify());

  const notification = {
    id: '0198f2c4-7d3a-7000-8000-000000000001',
    kind: 'escalation.assigned',
    state: 'unread',
    title: 'Escalation assigned',
    body: 'A customer needs help',
    subjectType: 'escalation',
    subjectId: '0198f2c4-7d3a-7000-8000-000000000002',
    actor: { membershipId: '0198f2c4-7d3a-7000-8000-000000000003', displayName: 'Dana Agent' },
    createdAt: '2026-11-11T10:00:00Z',
    readAt: null,
  };

  it('reads the camelCase notification list the backend sends', () => {
    let result: unknown;
    service.list().subscribe((response) => (result = response.data));

    http
      .expectOne((req) => req.url.endsWith('/tenant/notifications'))
      .flush({
        data: [notification],
        pagination: { next_cursor: 'abc', has_more: true },
      });

    expect(result).toEqual({
      items: [
        {
          id: notification.id,
          kind: 'escalation.assigned',
          state: 'unread',
          title: 'Escalation assigned',
          body: 'A customer needs help',
          subjectType: 'escalation',
          subjectId: notification.subjectId,
          actor: { membershipId: notification.actor.membershipId, displayName: 'Dana Agent' },
          createdAt: '2026-11-11T10:00:00Z',
          readAt: null,
        },
      ],
      hasMore: true,
      nextCursor: 'abc',
    });
  });

  it('treats an omitted body, actor and readAt as null', () => {
    let result: { items: { body: unknown; actor: unknown; readAt: unknown }[] } | undefined;
    service.list().subscribe((response) => (result = response.data));

    const { body, actor, readAt, ...bare } = notification;
    void body;
    void actor;
    void readAt;
    http
      .expectOne((req) => req.url.endsWith('/tenant/notifications'))
      .flush({ data: [bare], pagination: { next_cursor: null, has_more: false } });

    expect(result?.items[0]).toMatchObject({ body: null, actor: null, readAt: null });
  });

  it('reads the camelCase notification returned by mark-as-read', () => {
    let result: { subjectType: string; createdAt: string } | undefined;
    service.markRead(notification.id).subscribe((response) => (result = response.data));

    http
      .expectOne((req) => req.url.endsWith(`/tenant/notifications/${notification.id}/read`))
      .flush({ ...notification, state: 'read', readAt: '2026-11-11T10:05:00Z' });

    expect(result?.subjectType).toBe('escalation');
    expect(result?.createdAt).toBe('2026-11-11T10:00:00Z');
  });
});
