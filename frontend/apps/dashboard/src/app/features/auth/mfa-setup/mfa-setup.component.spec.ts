import { provideZonelessChangeDetection } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, provideRouter, Router } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { MfaService } from '../../../core/auth/mfa.service';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { MfaSetupComponent } from './mfa-setup.component';

const CODES = ['AAAAA-BBBBB', 'CCCCC-DDDDD'];

describe('MfaSetupComponent', () => {
  let mfa: {
    startEnrolment: ReturnType<typeof vi.fn>;
    confirmEnrolment: ReturnType<typeof vi.fn>;
  };
  let currentUser: { load: ReturnType<typeof vi.fn> };
  let fixture: ComponentFixture<MfaSetupComponent>;
  let router: Router;

  beforeEach(async () => {
    mfa = {
      startEnrolment: vi.fn().mockResolvedValue({
        secret: 'JBSWY3DPEHPK3PXP',
        otpauthUri: 'otpauth://totp/Helix:ann%40example.test?secret=JBSWY3DPEHPK3PXP',
      }),
      confirmEnrolment: vi.fn().mockResolvedValue(CODES),
    };
    currentUser = { load: vi.fn().mockResolvedValue(undefined) };
    await TestBed.configureTestingModule({
      imports: [MfaSetupComponent],
      providers: [
        provideRouter([]),
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: MfaService, useValue: mfa },
        { provide: CurrentUserService, useValue: currentUser },
      ],
    }).compileComponents();
    router = TestBed.inject(Router);
    vi.spyOn(router, 'navigateByUrl').mockResolvedValue(true);
    fixture = TestBed.createComponent(MfaSetupComponent);
    fixture.detectChanges();
  });

  const el = (): HTMLElement => fixture.nativeElement;
  const type = async (selector: string, value: string): Promise<void> => {
    const input = el().querySelector<HTMLInputElement>(selector)!;
    input.value = value;
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
  };
  const submit = async (): Promise<void> => {
    el().querySelector('form')!.dispatchEvent(new Event('submit'));
    await fixture.whenStable();
    fixture.detectChanges();
  };

  async function toCodeStep(): Promise<void> {
    await type('input[type="password"]', 'correct-horse');
    await submit();
  }

  it('starts with the password and does not continue without it', async () => {
    expect(el().textContent).toContain('Confirm your password');
    await submit();
    expect(mfa.startEnrolment).not.toHaveBeenCalled();
  });

  it('shows the setup key and the authenticator link after the password', async () => {
    await toCodeStep();
    expect(mfa.startEnrolment).toHaveBeenCalledWith('correct-horse');
    expect(el().querySelector('[data-testid="setup-key"]')!.textContent).toContain(
      'JBSWY3DPEHPK3PXP',
    );
    const link = el().querySelector<HTMLAnchorElement>('[data-testid="setup-link"]')!;
    expect(link.getAttribute('href')).toContain('otpauth://totp/');
  });

  it('shows no link when the server sent something that is not an authenticator link', async () => {
    mfa.startEnrolment.mockResolvedValue({ secret: 'ABC', otpauthUri: 'javascript:alert(1)' });
    await toCodeStep();
    expect(el().querySelector('[data-testid="setup-link"]')).toBeNull();
    expect(el().querySelector('[data-testid="setup-key"]')!.textContent).toContain('ABC');
  });

  it('says the password is wrong on a 422', async () => {
    mfa.startEnrolment.mockRejectedValue({ code: 'x', message: 'x', status: 422 });
    await toCodeStep();
    expect(el().textContent).toContain('The password is not correct.');
    expect(el().querySelector('[data-testid="setup-key"]')).toBeNull();
  });

  it('asks for the first code, then shows the recovery codes once', async () => {
    await toCodeStep();
    await type('input[inputmode="numeric"]', '123456');
    await submit();
    expect(mfa.confirmEnrolment).toHaveBeenCalledWith('123456');
    const codes = Array.from(el().querySelectorAll('[data-testid="recovery-codes"] code')).map(
      (c) => c.textContent,
    );
    expect(codes).toEqual(CODES);
    // The setup key is gone once it has served.
    expect(el().querySelector('[data-testid="setup-key"]')).toBeNull();
  });

  it('says the code is wrong on a 422 and stays on the code step', async () => {
    mfa.confirmEnrolment.mockRejectedValue({ code: 'x', message: 'x', status: 422 });
    await toCodeStep();
    await type('input[inputmode="numeric"]', '000000');
    await submit();
    expect(el().textContent).toContain('That code is not valid.');
    expect(el().querySelector('[data-testid="recovery-codes"]')).toBeNull();
  });

  it('will not continue until the recovery codes are saved, then reloads the profile and goes on', async () => {
    const route = TestBed.inject(ActivatedRoute);
    vi.spyOn(route.snapshot.queryParamMap, 'get').mockImplementation((key: string) =>
      key === 'returnUrl' ? '/tenant/team' : null,
    );
    await toCodeStep();
    await type('input[inputmode="numeric"]', '123456');
    await submit();

    const buttons = Array.from(el().querySelectorAll<HTMLButtonElement>('button'));
    const next = buttons.find((b) => b.textContent?.includes('Continue'))!;
    expect(next.disabled).toBe(true);

    const checkbox = el().querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    checkbox.click();
    fixture.detectChanges();
    expect(next.disabled).toBe(false);
    next.click();
    await fixture.whenStable();

    expect(currentUser.load).toHaveBeenCalled();
    expect(router.navigateByUrl).toHaveBeenCalledWith('/tenant/team');
  });

  it('copies the recovery codes', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    await toCodeStep();
    await type('input[inputmode="numeric"]', '123456');
    await submit();
    const copy = Array.from(el().querySelectorAll<HTMLButtonElement>('button')).find((b) =>
      b.textContent?.includes('Copy codes'),
    )!;
    copy.click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(writeText).toHaveBeenCalledWith(CODES.join('\n'));
    expect(el().textContent).toContain('Copied');
  });
});
