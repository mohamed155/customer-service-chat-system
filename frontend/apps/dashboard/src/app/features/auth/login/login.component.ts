import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { FormControl, FormGroup, ReactiveFormsModule, Validators } from '@angular/forms';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { AuthLoginError, AuthService } from '../../../core/auth/auth.service';
import { APP_PATHS } from '../../../core/router/app-paths';
import { AuthCardComponent } from '../auth-card/auth-card.component';

@Component({
  selector: 'app-login',
  imports: [AuthCardComponent, ReactiveFormsModule, RouterLink],
  template: `
    <app-auth-card title="Welcome back" subtitle="Sign in to your Helix workspace">
      <form [formGroup]="form" (ngSubmit)="submit()">
        <label>
          Email
          <input type="email" autocomplete="email" formControlName="email" />
        </label>
        <label>
          Password
          <input type="password" autocomplete="current-password" formControlName="password" />
        </label>
        @if (errorMessage()) {
          <p role="alert">{{ errorMessage() }}</p>
        }
        <button type="submit" [disabled]="pending() || form.invalid">
          {{ pending() ? 'Signing in...' : 'Sign in' }}
        </button>
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
  private readonly auth = inject(AuthService);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);

  protected readonly form = new FormGroup({
    email: new FormControl('', {
      nonNullable: true,
      validators: [Validators.required, Validators.email],
    }),
    password: new FormControl('', {
      nonNullable: true,
      validators: [Validators.required],
    }),
  });
  protected readonly pending = this.auth.pending;
  protected readonly errorMessage = signal<string | null>(null);
  protected readonly signupUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.signup}`;
  protected readonly forgotUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.forgotPassword}`;

  protected async submit(): Promise<void> {
    if (this.form.invalid) {
      this.form.markAllAsTouched();
      return;
    }

    this.errorMessage.set(null);
    const { email, password } = this.form.getRawValue();

    try {
      await this.auth.login(email, password);
      await this.router.navigateByUrl(this.safeReturnUrl());
    } catch (error) {
      this.errorMessage.set(error instanceof AuthLoginError ? error.message : 'Sign in failed');
    }
  }

  private safeReturnUrl(): string {
    const fallback = `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.overview}`;
    const returnUrl = this.route.snapshot.queryParamMap.get('returnUrl');
    if (!returnUrl || !returnUrl.startsWith('/') || returnUrl.startsWith('//')) return fallback;
    if (/^[a-z][a-z\d+.-]*:/i.test(returnUrl)) return fallback;
    return returnUrl;
  }
}
