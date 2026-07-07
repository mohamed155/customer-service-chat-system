import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../../core/router/app-paths';
import { AuthCardComponent } from '../auth-card/auth-card.component';

@Component({
  selector: 'app-verify-email',
  imports: [AuthCardComponent, RouterLink],
  template: `
    <app-auth-card title="Verify email" subtitle="Enter the six-digit code sent to your inbox">
      <form (submit)="$event.preventDefault()">
        <div class="otp" aria-label="Verification code">
          @for (box of boxes; track box) {
            <input
              inputmode="numeric"
              maxlength="1"
              [attr.aria-label]="'Digit ' + box"
              [class.active]="focusedIndex() === box - 1"
              (focus)="focusedIndex.set(box - 1)"
            />
          }
        </div>
        <button type="submit">Verify email</button>
      </form>
      <span auth-footer>
        Did not receive it? <a href="" (click)="$event.preventDefault()">Resend code</a>
        ·
        <a [routerLink]="loginUrl">Back to sign in</a>
      </span>
    </app-auth-card>
  `,
  styles: [
    `
      .otp {
        display: grid;
        grid-template-columns: repeat(6, 1fr);
        gap: var(--app-space-2);
      }
      .otp input {
        height: 48px;
        padding: 0;
        text-align: center;
        font: 700 var(--app-font-xl) / 1 var(--app-font-mono);
      }
      .otp input.active {
        border-color: var(--app-accent);
        box-shadow: 0 0 0 3px var(--app-accent-soft);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class VerifyEmailComponent {
  protected readonly boxes = [1, 2, 3, 4, 5, 6] as const;
  protected readonly focusedIndex = signal(0);
  protected readonly loginUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`;
}
