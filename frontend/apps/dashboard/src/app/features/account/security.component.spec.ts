import { provideZonelessChangeDetection } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { MfaService } from '../../core/auth/mfa.service';
import { SecurityComponent } from './security.component';

const status = (over: Partial<Record<string, unknown>> = {}) => ({
  enrolled: true,
  recoveryCodesRemaining: 7,
  required: true,
  sessionSecondFactor: true,
  ...over,
});

describe('SecurityComponent', () => {
  let mfa: {
    status: ReturnType<typeof vi.fn>;
    regenerateRecoveryCodes: ReturnType<typeof vi.fn>;
  };
  let fixture: ComponentFixture<SecurityComponent>;

  async function create(): Promise<void> {
    await TestBed.configureTestingModule({
      imports: [SecurityComponent],
      providers: [
        provideRouter([]),
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: MfaService, useValue: mfa },
      ],
    }).compileComponents();
    fixture = TestBed.createComponent(SecurityComponent);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
  }

  const el = (): HTMLElement => fixture.nativeElement;
  const button = (text: string): HTMLButtonElement =>
    Array.from(el().querySelectorAll<HTMLButtonElement>('button')).find((b) =>
      b.textContent?.includes(text),
    )!;

  beforeEach(() => {
    mfa = {
      status: vi.fn().mockResolvedValue(status()),
      regenerateRecoveryCodes: vi.fn().mockResolvedValue(['AAAAA-BBBBB', 'CCCCC-DDDDD']),
    };
  });

  it('says it is on and how many recovery codes are left', async () => {
    await create();
    expect(el().querySelector('[data-testid="enrolled"]')!.textContent).toContain(
      '7 recovery codes left',
    );
  });

  it('says it is off and links to set-up when the user has not enrolled', async () => {
    mfa.status.mockResolvedValue(status({ enrolled: false, recoveryCodesRemaining: 0 }));
    await create();
    expect(el().querySelector('[data-testid="not-enrolled"]')).toBeTruthy();
    expect(el().querySelector('a')!.getAttribute('href')).toBe('/mfa-setup');
    expect(button('Replace recovery codes')).toBeUndefined();
  });

  it('shows the new recovery codes once and can hide them again', async () => {
    await create();
    button('Replace recovery codes').click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(mfa.regenerateRecoveryCodes).toHaveBeenCalled();
    const codes = Array.from(el().querySelectorAll('[data-testid="new-codes"] code')).map(
      (c) => c.textContent,
    );
    expect(codes).toEqual(['AAAAA-BBBBB', 'CCCCC-DDDDD']);
    // The count is read again after the replacement.
    expect(mfa.status).toHaveBeenCalledTimes(2);

    button('I saved them').click();
    fixture.detectChanges();
    expect(el().querySelector('[data-testid="new-codes"]')).toBeNull();
  });

  it('stays quiet when the step-up prompt is cancelled', async () => {
    mfa.regenerateRecoveryCodes.mockRejectedValue({
      code: 'mfa_step_up_required',
      message: 'x',
      status: 401,
    });
    await create();
    button('Replace recovery codes').click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(el().querySelector('app-inline-alert')).toBeNull();
    expect(el().querySelector('[data-testid="new-codes"]')).toBeNull();
  });

  it('shows the request reference when the replacement fails', async () => {
    mfa.regenerateRecoveryCodes.mockRejectedValue({
      code: 'x',
      message: 'x',
      status: 500,
      requestId: 'req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f',
    });
    await create();
    button('Replace recovery codes').click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(el().querySelector('[data-testid="reference"]')?.textContent).toBe('3c4d5e6f');
  });

  it('says so when the replacement fails for another reason', async () => {
    mfa.regenerateRecoveryCodes.mockRejectedValue({ code: 'x', message: 'x', status: 500 });
    await create();
    button('Replace recovery codes').click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(el().textContent).toContain('Verification failed. Try again.');
  });
});
