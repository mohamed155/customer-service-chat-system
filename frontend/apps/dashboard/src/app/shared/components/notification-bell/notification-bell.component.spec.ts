import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { NotificationBellComponent } from './notification-bell.component';

describe('NotificationBellComponent', () => {
  async function setup(count: number = 0) {
    TestBed.configureTestingModule({
      imports: [NotificationBellComponent],
      providers: [provideTaiga()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(NotificationBellComponent);
    fixture.componentRef.setInput('count', count);
    fixture.detectChanges();
    return { fixture };
  }

  it('renders a bell button', async () => {
    const { fixture } = await setup();
    const btn = fixture.nativeElement.querySelector('.bell-button');
    expect(btn).toBeTruthy();
  });

  it('shows badge when count > 0', async () => {
    const { fixture } = await setup(3);
    const badge = fixture.nativeElement.querySelector('.badge');
    expect(badge).toBeTruthy();
    expect(badge.textContent?.trim()).toBe('3');
  });

  it('hides badge when count is 0', async () => {
    const { fixture } = await setup(0);
    const badge = fixture.nativeElement.querySelector('.badge');
    expect(badge).toBeNull();
  });

  it('caps badge text at 99+', async () => {
    const { fixture } = await setup(100);
    const badge = fixture.nativeElement.querySelector('.badge');
    expect(badge.textContent?.trim()).toBe('99+');
  });

  it('emits toggle on click', async () => {
    const { fixture } = await setup();
    const spy = vi.fn();
    fixture.componentRef.instance.togglePanel.subscribe(spy);
    fixture.nativeElement.querySelector('.bell-button').click();
    expect(spy).toHaveBeenCalledTimes(1);
  });
});
