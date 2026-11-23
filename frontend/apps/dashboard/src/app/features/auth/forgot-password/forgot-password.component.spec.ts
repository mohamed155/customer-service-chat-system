import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { Subject, of, throwError } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { PasswordResetService } from '../../../core/auth/password-reset.service';
import { ForgotPasswordComponent } from './forgot-password.component';

describe('ForgotPasswordComponent', () => {
  const forgot = vi.fn();

  async function setup() {
    TestBed.configureTestingModule({
      imports: [ForgotPasswordComponent],
      providers: [provideRouter([]), { provide: PasswordResetService, useValue: { forgot } }],
    });
    const fixture = TestBed.createComponent(ForgotPasswordComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;
    const email = element.querySelector('input')!;
    email.value = 'someone@example.com';
    email.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    return { fixture, element };
  }

  beforeEach(() => forgot.mockReset());

  it.each([{ data: null }, { data: { message: 'server wording' } }])(
    'shows neutral confirmation independently of the accepted body',
    async (response) => {
      forgot.mockReturnValue(of(response));
      const { fixture, element } = await setup();
      element.querySelector('button')!.click();
      await fixture.whenStable();
      expect(forgot).toHaveBeenCalledWith('someone@example.com');
      expect(element.querySelector('[role=status]')?.textContent).toContain(
        'If an account exists for that email',
      );
      expect(element.textContent).not.toContain('server wording');
    },
  );

  it('keeps the email form available after a rate limit', async () => {
    forgot.mockReturnValue(throwError(() => ({ status: 429, code: 'rate_limited' })));
    const { fixture, element } = await setup();
    element.querySelector('button')!.click();
    await fixture.whenStable();
    expect(element.querySelector('[role=alert]')?.textContent).toContain('Too many requests');
    expect(element.querySelector('button')?.disabled).toBe(false);
  });

  it('disables submission while the request is pending', async () => {
    const queued = new Subject<ApiResponse<void>>();
    forgot.mockReturnValue(queued);
    const { fixture, element } = await setup();
    const button = element.querySelector('button')!;
    button.click();
    fixture.detectChanges();
    expect(button.disabled).toBe(true);
    queued.next({ data: undefined });
    queued.complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(element.querySelector('[role=status]')).toBeTruthy();
    });
  });
});
