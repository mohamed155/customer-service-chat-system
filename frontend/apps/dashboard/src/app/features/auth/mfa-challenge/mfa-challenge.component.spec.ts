import { provideZonelessChangeDetection, signal, WritableSignal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, provideRouter, Router } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { AuthMfaError, AuthService } from '../../../core/auth/auth.service';
import { MfaChallengeComponent } from './mfa-challenge.component';

describe('MfaChallengeComponent', () => {
  let auth: { completeMfa: ReturnType<typeof vi.fn>; pending: WritableSignal<boolean> };
  let fixture: ComponentFixture<MfaChallengeComponent>;
  let router: Router;

  beforeEach(async () => {
    auth = { completeMfa: vi.fn(), pending: signal(false) };
    await TestBed.configureTestingModule({
      imports: [MfaChallengeComponent],
      providers: [
        provideRouter([]),
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: AuthService, useValue: auth },
      ],
    }).compileComponents();
    router = TestBed.inject(Router);
    vi.spyOn(router, 'navigateByUrl').mockResolvedValue(true);
    vi.spyOn(router, 'navigate').mockResolvedValue(true);
    fixture = TestBed.createComponent(MfaChallengeComponent);
    fixture.detectChanges();
  });

  const input = (): HTMLInputElement => fixture.nativeElement.querySelector('input');
  const submit = async (value: string): Promise<void> => {
    input().value = value;
    input().dispatchEvent(new Event('input'));
    fixture.detectChanges();
    fixture.nativeElement.querySelector('form').dispatchEvent(new Event('submit'));
    await fixture.whenStable();
    fixture.detectChanges();
  };
  const setReturnUrl = (returnUrl: string | null): void => {
    const route = fixture.debugElement.injector.get(ActivatedRoute);
    vi.spyOn(route.snapshot.queryParamMap, 'get').mockImplementation((key: string) =>
      key === 'returnUrl' ? returnUrl : null,
    );
    vi.spyOn(route.snapshot.queryParamMap, 'has').mockImplementation(
      (key: string) => key === 'returnUrl' && returnUrl != null,
    );
  };

  it('asks for the six-digit code first and offers the recovery code instead', () => {
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Two-step verification');
    expect(text).toContain('Authentication code');
    expect(text).toContain('Use a recovery code');
    expect(input().getAttribute('autocomplete')).toBe('one-time-code');
  });

  it('does not submit a code that is not six digits', async () => {
    await submit('123');
    expect(auth.completeMfa).not.toHaveBeenCalled();
  });

  it('sends the code and goes to the safe return URL', async () => {
    setReturnUrl('/tenant/conversations');
    auth.completeMfa.mockResolvedValue(undefined);
    await submit(' 123456 ');
    expect(auth.completeMfa).toHaveBeenCalledWith({ code: '123456' });
    expect(router.navigateByUrl).toHaveBeenCalledWith('/tenant/conversations');
  });

  it('ignores a return URL that leaves the app', async () => {
    setReturnUrl('//evil.example');
    auth.completeMfa.mockResolvedValue(undefined);
    await submit('123456');
    expect(router.navigateByUrl).toHaveBeenCalledWith('/tenant/overview');
  });

  it('switches to a recovery code and sends it as one', async () => {
    fixture.nativeElement.querySelector('button.link').click();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Recovery code');
    expect(fixture.nativeElement.textContent).toContain('Use an authenticator code');

    auth.completeMfa.mockResolvedValue(undefined);
    await submit('ABCDE-FGHJK');
    expect(auth.completeMfa).toHaveBeenCalledWith({ recoveryCode: 'ABCDE-FGHJK' });
  });

  it('shows the message for a wrong code and clears the field', async () => {
    auth.completeMfa.mockRejectedValue(
      new AuthMfaError('errors.codeInvalid', { code: 'x', message: 'x', status: 422 }, false),
    );
    await submit('123456');
    expect(fixture.nativeElement.textContent).toContain('That code is not valid.');
    expect(input().value).toBe('');
    expect(router.navigateByUrl).not.toHaveBeenCalled();
  });

  it('goes back to the password step when the challenge has expired', async () => {
    setReturnUrl('/tenant/team');
    auth.completeMfa.mockRejectedValue(
      new AuthMfaError('errors.signInExpired', { code: 'x', message: 'x', status: 401 }, true),
    );
    await submit('123456');
    expect(router.navigate).toHaveBeenCalledWith(['/auth/login'], {
      queryParams: { returnUrl: '/tenant/team' },
    });
  });

  it('disables the button while a check is in flight', () => {
    auth.pending.set(true);
    fixture.detectChanges();
    const button: HTMLButtonElement = fixture.nativeElement.querySelector('button[type="submit"]');
    expect(button.disabled).toBe(true);
    expect(button.textContent).toContain('Verifying...');
  });
});
