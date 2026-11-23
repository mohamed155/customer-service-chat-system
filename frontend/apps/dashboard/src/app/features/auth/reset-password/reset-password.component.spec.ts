import { TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { BehaviorSubject, of, Subject, throwError } from 'rxjs';
import { PasswordResetService } from '../../../core/auth/password-reset.service';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { ResetPasswordComponent } from './reset-password.component';

const TOKEN = 'A'.repeat(43);
const PASSWORD = 'new password';

describe('ResetPasswordComponent', () => {
  const reset = vi.fn();

  function setup(confirm = PASSWORD, token = TOKEN) {
    const params = new BehaviorSubject(convertToParamMap({ token }));
    TestBed.configureTestingModule({
      imports: [ResetPasswordComponent],
      providers: [
        provideRouter([]),
        {
          provide: ActivatedRoute,
          useValue: { paramMap: params },
        },
        { provide: PasswordResetService, useValue: { reset } },
        { provide: CurrentUserService, useValue: { currentUser: () => null } },
      ],
    });
    const fixture = TestBed.createComponent(ResetPasswordComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;
    const inputs = element.querySelectorAll('input');
    [PASSWORD, confirm].forEach((value, index) => {
      const input = inputs[index];
      if (input) {
        input.value = value;
        input.dispatchEvent(new Event('input'));
        input.dispatchEvent(new Event('blur'));
      }
    });
    fixture.detectChanges();
    return { fixture, element, params };
  }

  beforeEach(() => reset.mockReset());

  it('rejects a password confirmation that does not match', () => {
    const { element } = setup('different password');
    element.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));
    expect(reset).not.toHaveBeenCalled();
    expect(element.textContent).toContain('Passwords do not match');
  });

  it('offers a fresh link when the token is expired or invalid', async () => {
    reset.mockReturnValue(throwError(() => ({ status: 400, code: 'reset_token_invalid' })));
    const { fixture, element } = setup();
    element.querySelector('button')!.click();
    await fixture.whenStable();
    expect(element.querySelector('[role=status]')?.textContent).toContain('invalid or has expired');
    expect(element.querySelector('a[href="/auth/forgot-password"]')).toBeTruthy();
    expect(element.querySelector('form')).toBeNull();
  });

  it('treats a malformed link as invalid without submitting it', () => {
    const { element } = setup(PASSWORD, 'malformed');
    expect(element.querySelector('a[href="/auth/forgot-password"]')).toBeTruthy();
    expect(element.querySelector('form')).toBeNull();
    expect(reset).not.toHaveBeenCalled();
  });

  it('shows the translated password field error returned by the server', async () => {
    reset.mockReturnValue(
      throwError(() => ({
        status: 422,
        code: 'unprocessable',
        details: [{ field: 'password', code: 'too_long', message: 'server wording' }],
      })),
    );
    const { fixture, element } = setup();
    element.querySelector('button')!.click();
    await fixture.whenStable();
    expect(element.querySelector('#new-password-error')?.textContent).toContain('128 characters');
    expect(element.textContent).not.toContain('server wording');
  });

  it('starts fresh when the route changes while an earlier request is pending', async () => {
    const response = new Subject<{ data: null }>();
    reset.mockReturnValueOnce(response).mockReturnValueOnce(of({ data: null }));
    const { fixture, element, params } = setup();
    element.querySelector('button')!.click();
    const nextToken = 'B'.repeat(42) + 'A';
    params.next(convertToParamMap({ token: nextToken }));
    response.next({ data: null });
    response.complete();
    await Promise.resolve();
    fixture.detectChanges();
    expect(element.querySelector('[role=status]')).toBeNull();
    for (const input of element.querySelectorAll('input')) {
      expect(input.value).toBe('');
      input.value = PASSWORD;
      input.dispatchEvent(new Event('input'));
    }
    fixture.detectChanges();
    element.querySelector('button')!.click();
    await fixture.whenStable();
    expect(reset).toHaveBeenLastCalledWith(nextToken, PASSWORD);
    expect(element.querySelector('[role=status]')?.textContent).toContain(
      'Your password has changed',
    );
  });

  it('shows a sign-in link after a successful reset', async () => {
    reset.mockReturnValue(of({ data: null }));
    const { fixture, element } = setup();
    element.querySelector('button')!.click();
    await fixture.whenStable();
    expect(reset).toHaveBeenCalledWith(TOKEN, PASSWORD);
    expect(element.querySelector('[role=status]')?.textContent).toContain(
      'Your password has changed',
    );
    expect(element.querySelector('a[href="/auth/login"]')).toBeTruthy();
    expect(element.querySelector('form')).toBeNull();
  });
});
