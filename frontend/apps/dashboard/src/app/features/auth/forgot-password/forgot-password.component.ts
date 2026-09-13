import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../../core/router/app-paths';
import { AuthCardComponent } from '../auth-card/auth-card.component';

@Component({
  selector: 'app-forgot-password',
  imports: [AuthCardComponent, RouterLink],
  templateUrl: './forgot-password.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ForgotPasswordComponent {
  protected readonly loginUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`;
}
