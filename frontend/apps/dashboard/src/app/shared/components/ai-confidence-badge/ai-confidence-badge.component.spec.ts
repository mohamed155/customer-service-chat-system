import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { AiConfidenceBadgeComponent, ConfidenceBand } from './ai-confidence-badge.component';

describe('AiConfidenceBadgeComponent', () => {
  function createComponent(band: ConfidenceBand) {
    TestBed.configureTestingModule({
      imports: [AiConfidenceBadgeComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(AiConfidenceBadgeComponent);
    fixture.componentRef.setInput('band', band);
    fixture.detectChanges();
    return fixture;
  }

  it('renders a high confidence badge with green styling', () => {
    const fixture = createComponent('high');
    const el = fixture.nativeElement as HTMLElement;
    const badge = el.querySelector('.badge')!;
    expect(badge.textContent?.trim()).toBe('High');
    expect(badge.classList.contains('high')).toBe(true);
  });

  it('renders a medium confidence badge with amber styling', () => {
    const fixture = createComponent('medium');
    const el = fixture.nativeElement as HTMLElement;
    const badge = el.querySelector('.badge')!;
    expect(badge.textContent?.trim()).toBe('Medium');
    expect(badge.classList.contains('medium')).toBe(true);
  });

  it('renders a low confidence badge with red styling and bold weight', () => {
    const fixture = createComponent('low');
    const el = fixture.nativeElement as HTMLElement;
    const badge = el.querySelector('.badge')!;
    expect(badge.textContent?.trim()).toBe('Low');
    expect(badge.classList.contains('low')).toBe(true);
  });
});
