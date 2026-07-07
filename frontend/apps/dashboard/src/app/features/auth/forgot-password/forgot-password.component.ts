import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../../core/router/app-paths';
import { AuthCardComponent } from '../auth-card/auth-card.component';

@Component({
  selector: 'app-forgot-password',
  imports: [AuthCardComponent, RouterLink],
  template: `
    <app-auth-card title="Reset password" subtitle="Send a reset link to your workspace email">
      <form (submit)="$event.preventDefault()">
        <label>Email<input type="email" autocomplete="email" /></label>
        <button type="submit">Send reset link</button>
      </form>
      <span auth-footer><a [routerLink]="loginUrl">Back to sign in</a></span>
    </app-auth-card>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ForgotPasswordComponent {
  protected readonly loginUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`;
}
