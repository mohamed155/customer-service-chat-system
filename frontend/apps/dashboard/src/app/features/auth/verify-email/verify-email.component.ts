import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../../core/router/app-paths';
import { AuthCardComponent } from '../auth-card/auth-card.component';

@Component({
  selector: 'app-verify-email',
  imports: [AuthCardComponent, RouterLink],
  templateUrl: './verify-email.component.html',
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
