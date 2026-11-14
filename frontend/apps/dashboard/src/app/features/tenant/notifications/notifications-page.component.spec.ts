import { provideZonelessChangeDetection, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { NotificationsStore } from '../../../core/notifications/notifications.store';
import { NotificationsPageComponent } from './notifications-page.component';

describe('NotificationsPageComponent', () => {
  function render(unread: number) {
    const store = {
      items: signal([]),
      loading: signal(false),
      hasMore: signal(false),
      unreadCount: signal(unread),
      loadFirstPage: vi.fn(),
      markAllRead: vi.fn(),
      markRead: vi.fn(),
      loadMore: vi.fn(),
    };
    TestBed.configureTestingModule({
      imports: [NotificationsPageComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideRouter([]),
        { provide: NotificationsStore, useValue: store },
      ],
    });
    const fixture = TestBed.createComponent(NotificationsPageComponent);
    fixture.detectChanges();
    return { fixture, store, el: fixture.nativeElement as HTMLElement };
  }

  it('titles the page and loads the first page', () => {
    const { store, el } = render(0);
    expect(el.querySelector('h2')?.textContent?.trim()).toBe('Notifications');
    expect(store.loadFirstPage).toHaveBeenCalled();
    expect(el.querySelector('.mark-all-btn')).toBeNull();
  });

  it('offers to mark everything read while there is something unread', () => {
    const { store, el } = render(3);
    const button = el.querySelector('.mark-all-btn') as HTMLButtonElement;
    expect(button.textContent?.trim()).toBe('Mark all as read');
    button.click();
    expect(store.markAllRead).toHaveBeenCalled();
  });
});
