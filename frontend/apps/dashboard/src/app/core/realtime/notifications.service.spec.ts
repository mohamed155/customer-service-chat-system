/* eslint-disable @typescript-eslint/no-explicit-any */
import { TestBed } from '@angular/core/testing';
import { Subject, of } from 'rxjs';
import { provideZonelessChangeDetection } from '@angular/core';
import { RealtimeService, SseEvent } from './realtime.service';
import { NotificationsService } from './notifications.service';
import { NotificationsStore } from '../notifications/notifications.store';
import { NotificationsApiService } from '../notifications/notifications.api';

describe('NotificationsService', () => {
  let origNotification: typeof Notification;
  let origHidden: PropertyDescriptor | undefined;

  function buildService() {
    const subj = new Subject<SseEvent>();
    TestBed.configureTestingModule({
      providers: [
        provideZonelessChangeDetection(),
        NotificationsService,
        NotificationsStore,
        { provide: RealtimeService, useValue: { events: () => subj.asObservable() } },
        {
          provide: NotificationsApiService,
          useValue: {
            list: () => of({ data: { items: [], hasMore: false, nextCursor: null } }),
            unreadCount: () => of({ data: { count: 0 } }),
            markRead: () => of({ data: {} as any }),
            markAllRead: () => of({ data: { marked: 0 } }),
          },
        },
      ],
    });
    return { service: TestBed.inject(NotificationsService), subject: subj, store: TestBed.inject(NotificationsStore) };
  }

  beforeEach(() => {
    origNotification = globalThis.Notification;
    origHidden = Object.getOwnPropertyDescriptor(Document.prototype, 'hidden');
  });

  afterEach(() => {
    (globalThis as any).Notification = origNotification;
    if (origHidden) {
      Object.defineProperty(document, 'hidden', origHidden);
    }
  });

  it('requests permission only on explicit caller action, never in constructor', () => {
    const requestPermission = vi.fn().mockResolvedValue('granted');
    (globalThis as any).Notification = { permission: 'default', requestPermission };
    const { service } = buildService();
    expect(requestPermission).not.toHaveBeenCalled();
    service.requestPermission();
    expect(requestPermission).toHaveBeenCalledTimes(1);
  });

  it('updates store unreadCount on notification.created', () => {
    const { subject, store } = buildService();
    expect(store.unreadCount()).toBe(0);
    subject.next({
      event: 'notification.created',
      id: '1',
      data: JSON.stringify({ membershipId: 'mem-1', notificationId: 'n-1', unreadCount: 5 }),
    });
    expect(store.unreadCount()).toBe(5);
  });

  it('updates store unreadCount on notification.cleared', () => {
    const { subject, store } = buildService();
    subject.next({
      event: 'notification.cleared',
      id: '2',
      data: JSON.stringify({ membershipId: 'mem-1', unreadCount: 2 }),
    });
    expect(store.unreadCount()).toBe(2);
  });

  it('sends browser notification only when permission granted and document is hidden', () => {
    const notify = vi.fn();
    (globalThis as any).Notification = function MockNotification(
      title: string,
      opts?: NotificationOptions,
    ) {
      notify(title, opts);
    };
    (globalThis as any).Notification.permission = 'granted';
    Object.defineProperty(document, 'hidden', { configurable: true, value: true });

    const { subject } = buildService();
    subject.next({
      event: 'notification.created',
      id: '1',
      data: JSON.stringify({ membershipId: 'mem-1', notificationId: 'n-1', unreadCount: 1 }),
    });
    expect(notify).toHaveBeenCalledWith('New notification', expect.objectContaining({}));
  });

  it('does not send browser notification when document is visible', () => {
    const notify = vi.fn();
    (globalThis as any).Notification = function MockNotification(
      title: string,
      opts?: NotificationOptions,
    ) {
      notify(title, opts);
    };
    (globalThis as any).Notification.permission = 'granted';
    Object.defineProperty(document, 'hidden', { configurable: true, value: false });

    const { subject } = buildService();
    subject.next({
      event: 'notification.created',
      id: '1',
      data: JSON.stringify({ membershipId: 'mem-1', notificationId: 'n-1', unreadCount: 1 }),
    });
    expect(notify).not.toHaveBeenCalled();
  });

  it('does not throw when Notification is unsupported', () => {
    (globalThis as any).Notification = undefined;
    const { subject } = buildService();
    expect(() => {
      subject.next({
        event: 'notification.created',
        id: '1',
        data: JSON.stringify({ membershipId: 'mem-1', notificationId: 'n-1', unreadCount: 1 }),
      });
    }).not.toThrow();
  });

  it('ignores non-notification events', () => {
    const { subject, store } = buildService();
    subject.next({ event: 'escalation.assigned', id: '1', data: '{}' });
    expect(store.unreadCount()).toBe(0);
  });
});
