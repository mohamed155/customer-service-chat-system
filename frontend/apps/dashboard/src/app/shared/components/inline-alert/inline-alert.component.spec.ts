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
  describe('request reference (MOH-64)', () => {
    const show = (tone: 'error' | 'info', requestId: string | null) => {
      const fixture = TestBed.createComponent(InlineAlertComponent);
      fixture.componentRef.setInput('tone', tone);
      fixture.componentRef.setInput('requestId', requestId);
      fixture.detectChanges();
      return fixture.nativeElement as HTMLElement;
    };

    it('shows the short reference of the failed request on an error', () => {
      const el = show('error', 'req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f');
      expect(el.querySelector('[data-testid="reference"]')?.textContent).toBe('3c4d5e6f');
    });

    it('shows none when the error has no request ID', () => {
      expect(show('error', null).querySelector('app-error-reference')).toBeNull();
    });

    it('shows none on a message that is not an error', () => {
      expect(
        show('info', 'req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f').querySelector(
          'app-error-reference',
        ),
      ).toBeNull();
    });
  });
});
