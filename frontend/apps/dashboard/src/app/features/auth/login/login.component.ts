import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../../core/router/app-paths';
import { AuthCardComponent } from '../auth-card/auth-card.component';

@Component({
  selector: 'app-login',
  imports: [AuthCardComponent, RouterLink],
  template: `
    <app-auth-card title="Welcome back" subtitle="Sign in to your Helix workspace">
      <form (submit)="$event.preventDefault()">
        <label>Email<input type="email" autocomplete="email" /></label>
        <label>Password<input type="password" autocomplete="current-password" /></label>
        <button type="submit">Sign in</button>
      </form>
      <span auth-footer>
        <a [routerLink]="forgotUrl">Forgot password?</a>
        ·
        <a [routerLink]="signupUrl">Create account</a>
      </span>
    </app-auth-card>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LoginComponent {
  protected readonly signupUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.signup}`;
  protected readonly forgotUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.forgotPassword}`;
}
