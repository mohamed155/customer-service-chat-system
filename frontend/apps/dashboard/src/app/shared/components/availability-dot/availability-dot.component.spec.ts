import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { AvailabilityDotComponent } from './availability-dot.component';

describe('AvailabilityDotComponent', () => {
  async function createComponent(state: 'available' | 'away') {
    TestBed.configureTestingModule({
      imports: [AvailabilityDotComponent],
      providers: [provideZonelessChangeDetection()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AvailabilityDotComponent);
    fixture.componentRef.setInput('state', state);
    fixture.detectChanges();
    return fixture;
  }

  it('shows available visual and aria state', async () => {
    const fixture = await createComponent('available');
    const el = fixture.nativeElement.querySelector('.dot') as HTMLElement;

    expect(el.classList.contains('available')).toBe(true);
    expect(el.classList.contains('away')).toBe(false);
    expect(el.getAttribute('aria-label')).toBe('Available');
    expect(el.querySelector('tui-icon')).toBeTruthy();
  });

  it('shows away visual and aria state', async () => {
    const fixture = await createComponent('away');
    const el = fixture.nativeElement.querySelector('.dot') as HTMLElement;

    expect(el.classList.contains('away')).toBe(true);
    expect(el.classList.contains('available')).toBe(false);
    expect(el.getAttribute('aria-label')).toBe('Away');
    expect(el.querySelector('tui-icon')).toBeTruthy();
  });
});
