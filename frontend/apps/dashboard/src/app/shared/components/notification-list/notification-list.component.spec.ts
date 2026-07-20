import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { NOTIFICATION_FIXTURES } from '../../fixtures/notification.fixtures';
import { NotificationListComponent } from './notification-list.component';

describe('NotificationListComponent', () => {
  async function setup(
    overrides: {
      items?: typeof NOTIFICATION_FIXTURES;
      loading?: boolean;
      hasMore?: boolean;
    } = {},
  ) {
    TestBed.configureTestingModule({
      imports: [NotificationListComponent],
      providers: [provideTaiga()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(NotificationListComponent);
    fixture.componentRef.setInput('items', overrides.items ?? NOTIFICATION_FIXTURES);
    if (overrides.loading !== undefined) {
      fixture.componentRef.setInput('loading', overrides.loading);
    }
    if (overrides.hasMore !== undefined) {
      fixture.componentRef.setInput('hasMore', overrides.hasMore);
    }
    fixture.detectChanges();
    return { fixture };
  }

  it('renders one row per notification', async () => {
    const { fixture } = await setup();
    const items = fixture.nativeElement.querySelectorAll('.item');
    expect(items.length).toBe(NOTIFICATION_FIXTURES.length);
  });

  it('shows loading state when loading and no items', async () => {
    const { fixture } = await setup({ items: [], loading: true });
    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
  });

  it('shows empty state when no items and not loading', async () => {
    const { fixture } = await setup({ items: [] });
    expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
  });

  it('emits itemClick on item click', async () => {
    const { fixture } = await setup();
    const spy = vi.fn();
    fixture.componentRef.instance.itemClick.subscribe(spy);
    const firstItem = fixture.nativeElement.querySelector('.item');
    firstItem.click();
    expect(spy).toHaveBeenCalledWith(NOTIFICATION_FIXTURES[0]);
  });

  it('emits markRead on mark read button click', async () => {
    const { fixture } = await setup({
      items: [NOTIFICATION_FIXTURES[0]], // only unread items have the button
    });
    const spy = vi.fn();
    fixture.componentRef.instance.markRead.subscribe(spy);
    const markBtn = fixture.nativeElement.querySelector('.mark-read-btn');
    markBtn.click();
    expect(spy).toHaveBeenCalledWith(NOTIFICATION_FIXTURES[0].id);
  });

  it('emits loadMore on load more button click', async () => {
    const { fixture } = await setup({ hasMore: true });
    const spy = vi.fn();
    fixture.componentRef.instance.loadMore.subscribe(spy);
    const loadBtn = fixture.nativeElement.querySelector('.load-more');
    loadBtn.click();
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('hides load more button when hasMore is false', async () => {
    const { fixture } = await setup({ hasMore: false });
    expect(fixture.nativeElement.querySelector('.load-more')).toBeNull();
  });

  it('applies state-unread class for unread items', async () => {
    const { fixture } = await setup();
    const unreadItem = fixture.nativeElement.querySelector('.state-unread');
    expect(unreadItem).toBeTruthy();
  });
});
