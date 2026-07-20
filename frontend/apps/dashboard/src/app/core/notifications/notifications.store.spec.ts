import { TestBed } from '@angular/core/testing';
import { provideZonelessChangeDetection } from '@angular/core';
import { of } from 'rxjs';
import { NotificationsStore } from './notifications.store';
import { NotificationsApiService } from './notifications.api';
import { NotificationEntry } from '../api/tenant-api.models';

describe('NotificationsStore', () => {
  function createStore(mockApi?: Partial<NotificationsApiService>) {
    TestBed.configureTestingModule({
      providers: [
        provideZonelessChangeDetection(),
        NotificationsStore,
        {
          provide: NotificationsApiService,
          useValue: {
            list: () => of({ data: { items: [], hasMore: false, nextCursor: null } }),
            unreadCount: () => of({ data: { count: 0 } }),
            markRead: (id: string) =>
              of({
                data: {
                  id,
                  kind: '',
                  state: 'read',
                  title: '',
                  body: null,
                  subjectType: '',
                  subjectId: '',
                  actor: null,
                  createdAt: '',
                  readAt: new Date().toISOString(),
                },
              }),
            markAllRead: () => of({ data: { marked: 0 } }),
            ...mockApi,
          },
        },
      ],
    });
    return TestBed.inject(NotificationsStore);
  }

  it('setUnreadCount is assignment-only — two successive calls leave count at 3, not 6', () => {
    const store = createStore();
    expect(store.unreadCount()).toBe(0);
    store.setUnreadCount(3);
    expect(store.unreadCount()).toBe(3);
    store.setUnreadCount(3);
    expect(store.unreadCount()).toBe(3);
  });
});
