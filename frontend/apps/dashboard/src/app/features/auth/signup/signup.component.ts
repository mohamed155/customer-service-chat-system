import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../../core/router/app-paths';
import { AuthCardComponent } from '../auth-card/auth-card.component';

@Component({
  selector: 'app-signup',
  imports: [AuthCardComponent, RouterLink],
  template: `
    <app-auth-card title="Create workspace" subtitle="Start a Helix Support AI workspace">
      <form (submit)="$event.preventDefault()">
        <label>Name<input autocomplete="name" /></label>
        <label>Email<input type="email" autocomplete="email" /></label>
        <label>Password<input type="password" autocomplete="new-password" /></label>
        <button type="submit">Create account</button>
      </form>
      <span auth-footer>
        By continuing you agree to the workspace terms. Already have an account?
        <a [routerLink]="loginUrl">Sign in</a>
      </span>
    </app-auth-card>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SignupComponent {
  protected readonly loginUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`;
}
