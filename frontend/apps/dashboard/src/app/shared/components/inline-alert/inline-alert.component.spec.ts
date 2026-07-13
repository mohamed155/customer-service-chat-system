import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { InlineAlertComponent } from './inline-alert.component';

describe('InlineAlertComponent', () => {
  beforeEach(() => {
    TestBed.configureTestingModule({
      imports: [InlineAlertComponent],
      providers: [provideZonelessChangeDetection()],
    });
  });

  it('announces errors assertively', () => {
    const fixture = TestBed.createComponent(InlineAlertComponent);
    fixture.componentRef.setInput('tone', 'error');
    fixture.detectChanges();

    const alert = fixture.nativeElement.querySelector('p');
    expect(alert.getAttribute('role')).toBe('alert');
    expect(alert.getAttribute('aria-live')).toBe('assertive');
  });

  it('announces informational updates politely', () => {
    const fixture = TestBed.createComponent(InlineAlertComponent);
    fixture.componentRef.setInput('tone', 'info');
    fixture.detectChanges();

    const alert = fixture.nativeElement.querySelector('p');
    expect(alert.getAttribute('role')).toBe('status');
    expect(alert.getAttribute('aria-live')).toBe('polite');
  });
});
