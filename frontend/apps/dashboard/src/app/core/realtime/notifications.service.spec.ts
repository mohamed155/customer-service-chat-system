/* eslint-disable @typescript-eslint/no-explicit-any */
import { TestBed } from '@angular/core/testing';
import { Subject } from 'rxjs';
import { provideZonelessChangeDetection } from '@angular/core';
import { RealtimeService, SseEvent } from './realtime.service';
import { NotificationsService } from './notifications.service';

describe('NotificationsService', () => {
  let origNotification: typeof Notification;
  let origHidden: PropertyDescriptor | undefined;

  function buildService() {
    const subj = new Subject<SseEvent>();
    TestBed.configureTestingModule({
      providers: [
        provideZonelessChangeDetection(),
        NotificationsService,
        { provide: RealtimeService, useValue: { events: () => subj.asObservable() } },
      ],
    });
    return { service: TestBed.inject(NotificationsService), subject: subj };
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

  it('increments in-app signal on escalation.assigned regardless of Notification permission', () => {
    (globalThis as any).Notification = { permission: 'denied' };
    const { service, subject } = buildService();
    expect(service.inAppSignal()).toBe(0);
    subject.next({ event: 'escalation.assigned', id: '1', data: '{}' });
    expect(service.inAppSignal()).toBe(1);
    subject.next({ event: 'escalation.assigned', id: '2', data: '{}' });
    expect(service.inAppSignal()).toBe(2);
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
      event: 'escalation.assigned',
      id: '1',
      data: JSON.stringify({ reason: 'skill match' }),
    });
    expect(notify).toHaveBeenCalledWith(
      'Escalation assigned',
      expect.objectContaining({ body: 'skill match' }),
    );
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
    subject.next({ event: 'escalation.assigned', id: '1', data: '{}' });
    expect(notify).not.toHaveBeenCalled();
  });

  it('does not throw when Notification is unsupported or permission is denied', () => {
    (globalThis as any).Notification = undefined;
    const { subject } = buildService();
    expect(() => {
      subject.next({ event: 'escalation.assigned', id: '1', data: '{}' });
    }).not.toThrow();

    (globalThis as any).Notification = { permission: 'denied' };
    expect(() => {
      subject.next({ event: 'escalation.assigned', id: '2', data: '{}' });
    }).not.toThrow();
  });

  it('does not throw when JSON.parse fails in browser notification path', () => {
    (globalThis as any).Notification = function MockNotification() {};
    (globalThis as any).Notification.permission = 'granted';
    Object.defineProperty(document, 'hidden', { configurable: true, value: true });

    const { subject } = buildService();
    expect(() => {
      subject.next({ event: 'escalation.assigned', id: '1', data: 'not-json' });
    }).not.toThrow();
  });
});
