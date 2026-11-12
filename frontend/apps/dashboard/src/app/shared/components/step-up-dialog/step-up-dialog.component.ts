import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { FormControl, ReactiveFormsModule, Validators } from '@angular/forms';
import { StepUpService } from '../../../core/auth/step-up.service';
import { ButtonComponent } from '../button/button.component';
import { DialogShellComponent } from '../dialog-shell/dialog-shell.component';
import { InlineAlertComponent } from '../inline-alert/inline-alert.component';

const CODE_PATTERN = /^\s*\d{6}\s*$/;
const RECOVERY_PATTERN = /^[A-Za-z0-9\s-]{10,16}$/;

/** The prompt for a fresh code. Mounted once, in the app root; shown by `StepUpService`. */
@Component({
  selector: 'app-step-up-dialog',
  imports: [ButtonComponent, DialogShellComponent, InlineAlertComponent, ReactiveFormsModule],
  templateUrl: './step-up-dialog.component.html',
  styles: [
    `
      h3 {
        margin: 0 0 0.5rem;
        font-size: 1.125rem;
      }
      label {
        display: grid;
        gap: 0.25rem;
        margin: 1rem 0;
      }
      .actions {
        display: flex;
        gap: 0.5rem;
        justify-content: flex-end;
        align-items: center;
      }
      .link {
        margin-right: auto;
        border: 0;
        background: none;
        color: var(--app-accent);
        cursor: pointer;
        font: inherit;
        padding: 0;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class StepUpDialogComponent {
  protected readonly stepUp = inject(StepUpService);

  protected readonly credential = new FormControl('', {
    nonNullable: true,
    validators: [Validators.required, Validators.pattern(CODE_PATTERN)],
  });
  protected readonly usingRecovery = signal(false);

  protected toggleRecovery(): void {
    const recovery = !this.usingRecovery();
    this.usingRecovery.set(recovery);
    this.credential.setValidators([
      Validators.required,
      Validators.pattern(recovery ? RECOVERY_PATTERN : CODE_PATTERN),
    ]);
    this.credential.reset('');
  }

  protected async submit(): Promise<void> {
    if (this.credential.invalid) {
      this.credential.markAsTouched();
      return;
    }
    const value = this.credential.getRawValue().trim();
    await this.stepUp.submit(this.usingRecovery() ? { recoveryCode: value } : { code: value });
    if (!this.stepUp.open()) {
      this.credential.reset('');
    }
  }

  protected cancel(): void {
    this.credential.reset('');
    this.stepUp.cancel();
  }
}
