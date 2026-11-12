import { provideZonelessChangeDetection, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { StepUpService } from '../../../core/auth/step-up.service';
import { StepUpDialogComponent } from './step-up-dialog.component';

describe('StepUpDialogComponent', () => {
  let stepUp: {
    open: ReturnType<typeof signal<boolean>>;
    pending: ReturnType<typeof signal<boolean>>;
    errorMessage: ReturnType<typeof signal<string | null>>;
    submit: ReturnType<typeof vi.fn>;
    cancel: ReturnType<typeof vi.fn>;
  };
  let fixture: ComponentFixture<StepUpDialogComponent>;

  beforeEach(async () => {
    stepUp = {
      open: signal(false),
      pending: signal(false),
      errorMessage: signal<string | null>(null),
      submit: vi.fn().mockResolvedValue(undefined),
      cancel: vi.fn(),
    };
    await TestBed.configureTestingModule({
      imports: [StepUpDialogComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: StepUpService, useValue: stepUp },
      ],
    }).compileComponents();
    fixture = TestBed.createComponent(StepUpDialogComponent);
    fixture.detectChanges();
  });

  const el = (): HTMLElement => fixture.nativeElement;
  const open = (): void => {
    stepUp.open.set(true);
    fixture.detectChanges();
  };
  const type = (value: string): void => {
    const input = el().querySelector<HTMLInputElement>('input')!;
    input.value = value;
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
  };

  it('shows nothing until the service opens the prompt', () => {
    expect(el().querySelector('input')).toBeNull();
    open();
    expect(el().textContent).toContain('Confirm it is you');
    expect(el().textContent).toContain('Authentication code');
  });

  it('sends the code to the service', async () => {
    open();
    type('123456');
    el().querySelector('form')!.dispatchEvent(new Event('submit'));
    await fixture.whenStable();
    expect(stepUp.submit).toHaveBeenCalledWith({ code: '123456' });
  });

  it('does not send a code that is not six digits', async () => {
    open();
    type('12');
    el().querySelector('form')!.dispatchEvent(new Event('submit'));
    await fixture.whenStable();
    expect(stepUp.submit).not.toHaveBeenCalled();
  });

  it('can use a recovery code instead', async () => {
    open();
    const toggle = el().querySelector<HTMLButtonElement>('button.link')!;
    toggle.click();
    fixture.detectChanges();
    expect(el().textContent).toContain('Recovery code');
    type('ABCDE-FGHJK');
    el().querySelector('form')!.dispatchEvent(new Event('submit'));
    await fixture.whenStable();
    expect(stepUp.submit).toHaveBeenCalledWith({ recoveryCode: 'ABCDE-FGHJK' });
  });

  it('shows why a code was refused', () => {
    open();
    stepUp.errorMessage.set('That code is not valid.');
    fixture.detectChanges();
    expect(el().textContent).toContain('That code is not valid.');
  });

  it('cancels through the service', () => {
    open();
    const cancel = Array.from(el().querySelectorAll<HTMLButtonElement>('button')).find((b) =>
      b.textContent?.includes('Cancel'),
    )!;
    cancel.click();
    expect(stepUp.cancel).toHaveBeenCalled();
  });
});
