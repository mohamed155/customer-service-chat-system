import { DatePipe } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { FormControl, FormGroup, ReactiveFormsModule, Validators } from '@angular/forms';
import { ActivatedRoute, Router } from '@angular/router';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { catchError, EMPTY, from, map, pipe, switchMap, tap } from 'rxjs';
import { AcceptInvitationRequest, InvitationPreview } from '../../../core/api/tenant-api.models';
import { APP_PATHS } from '../../../core/router/app-paths';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { AuthService } from '../../../core/auth/auth.service';
import { PAGE_PERMISSIONS } from '../../../core/authz/permissions';
import { AuthCardComponent } from '../auth-card/auth-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { TeamApiService } from '../../tenant/team/team-api.service';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';

type InvitePageStatus = 'loading' | 'preview' | 'form' | 'submitting' | 'success' | 'error';
type InviteRecoveryAction =
  'login' | 'switchAccount' | 'freshInvitation' | 'reenableMembership' | null;

const TENANT_LANDING_ORDER = [
  APP_PATHS.tenant.overview,
  APP_PATHS.tenant.conversations,
  APP_PATHS.tenant.customers,
  APP_PATHS.tenant.aiAgent,
  APP_PATHS.tenant.knowledgeBase,
  APP_PATHS.tenant.integrations,
  APP_PATHS.tenant.analytics,
  APP_PATHS.tenant.settings,
] as const;

@Component({
  selector: 'app-accept-invitation',
  imports: [
    DatePipe,
    ReactiveFormsModule,
    AuthCardComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    ButtonComponent,
    DashboardCardComponent,
    FormFieldComponent,
    InlineAlertComponent,
  ],
  template: `
    <app-auth-card title="Accept invitation" [subtitle]="cardSubtitle()">
      @switch (status()) {
        @case ('loading') {
          <app-loading-state label="Verifying invitation…" />
        }
        @case ('preview') {
          <app-dashboard-card>
            <div class="row">
              <span>Email</span><strong>{{ preview()?.email }}</strong>
            </div>
            <div class="row">
              <span>Role</span><strong>{{ preview()?.role }}</strong>
            </div>
            <div class="row">
              <span>Expires</span><strong>{{ preview()?.expiresAt | date: 'mediumDate' }}</strong>
            </div>
          </app-dashboard-card>
          @if (signedInEmailMismatch()) {
            <app-inline-alert tone="error">
              This invitation was issued to {{ preview()?.email }}. Sign out and use that email to
              accept it.
            </app-inline-alert>
            <app-button variant="primary" (pressed)="signOut()"
              >Sign out and switch account</app-button
            >
          } @else if (isAuthenticated()) {
            <p class="info">You’re already signed in. Accept this invitation to join the team.</p>
            <app-button variant="primary" (pressed)="acceptSignedIn()"
              >Accept invitation</app-button
            >
          } @else {
            <p class="info">You already have an account. Sign in to accept this invitation.</p>
            <app-button variant="primary" (pressed)="goToLogin()">Sign in</app-button>
          }
        }
        @case ('form') {
          @if (signedInEmailMismatch()) {
            <app-inline-alert tone="error">
              This invitation was issued to {{ preview()?.email }}. Sign out and use that email to
              accept it.
            </app-inline-alert>
            <app-button variant="primary" (pressed)="signOut()">Sign out</app-button>
          } @else {
            <app-dashboard-card>
              <p>
                <strong>{{ preview()?.email }}</strong> will be invited as
                <strong>{{ preview()?.role }}</strong
                >.
              </p>
            </app-dashboard-card>
            <form class="form-fields" [formGroup]="form" (ngSubmit)="submit()">
              <app-form-field label="Display name" for="invite-display-name">
                <input
                  id="invite-display-name"
                  type="text"
                  placeholder="Your full name"
                  formControlName="displayName"
                  [attr.aria-invalid]="
                    form.controls.displayName.invalid && form.controls.displayName.touched
                  "
                  [attr.aria-describedby]="
                    controlError('displayName') ? 'display-name-error' : null
                  "
                />
              </app-form-field>
              @if (controlError('displayName'); as err) {
                <app-inline-alert id="display-name-error" tone="error">{{ err }}</app-inline-alert>
              }
              <app-form-field label="Password" for="invite-password">
                <input
                  id="invite-password"
                  type="password"
                  placeholder="Create a password (min 8 characters)"
                  formControlName="password"
                  [attr.aria-invalid]="
                    form.controls.password.invalid && form.controls.password.touched
                  "
                  [attr.aria-describedby]="controlError('password') ? 'password-error' : null"
                />
              </app-form-field>
              @if (controlError('password'); as err) {
                <app-inline-alert id="password-error" tone="error">{{ err }}</app-inline-alert>
              }
              @if (errorMessage(); as err) {
                <app-inline-alert tone="error">{{ err }}</app-inline-alert>
              }
              @if (recoveryAction() === 'switchAccount') {
                <app-button variant="primary" (pressed)="signOut()">Sign out</app-button>
              } @else if (recoveryAction() === 'login') {
                <app-button variant="primary" (pressed)="goToLogin()">Sign in</app-button>
              } @else if (recoveryAction() === 'freshInvitation') {
                <p class="info">Ask a workspace admin to issue a fresh invitation.</p>
              } @else if (recoveryAction() === 'reenableMembership') {
                <p class="info">
                  Ask a workspace Owner or Admin to re-enable your existing membership. A new
                  invitation cannot restore disabled access.
                </p>
              }
              <app-button variant="primary" type="submit" [disabled]="status() === 'submitting'">
                {{ status() === 'submitting' ? 'Accepting…' : 'Accept & join' }}
              </app-button>
            </form>
          }
        }
        @case ('submitting') {
          <app-loading-state label="Accepting invitation…" />
        }
        @case ('success') {
          <app-empty-state
            icon="@tui.check"
            title="Welcome aboard!"
            description="Your account has been created. You can now sign in and start collaborating."
          >
            <app-button variant="primary" (pressed)="goToLogin()">Sign in</app-button>
          </app-empty-state>
        }
        @case ('error') {
          <app-empty-state
            icon="@tui.alert-circle"
            title="Invitation issue"
            [description]="errorMessage()"
          >
            @if (recoveryAction() === 'switchAccount') {
              <app-button variant="primary" (pressed)="signOut()"
                >Sign out and switch account</app-button
              >
            } @else if (recoveryAction() === 'login') {
              <app-button variant="primary" (pressed)="goToLogin()">Sign in</app-button>
            } @else if (recoveryAction() === 'freshInvitation') {
              <p class="info">Ask a workspace admin to issue a fresh invitation.</p>
            } @else if (recoveryAction() === 'reenableMembership') {
              <p class="info">
                Ask a workspace Owner or Admin to re-enable your existing membership. A new
                invitation cannot restore disabled access.
              </p>
            }
          </app-empty-state>
        }
      }
    </app-auth-card>
  `,
  styles: [
    `
      app-dashboard-card {
        margin-bottom: 1rem;
        max-width: 480px;
      }
      .row {
        display: flex;
        justify-content: space-between;
        padding: 0.375rem 0;
      }
      .row + .row {
        border-top: 1px solid var(--tui-border-normal);
      }
      .form-fields {
        display: grid;
        gap: 1rem;
        max-width: 480px;
        margin-bottom: 1rem;
      }
      app-form-field {
        display: block;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AcceptInvitationComponent {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly api = inject(TeamApiService);
  private readonly currentUserService = inject(CurrentUserService);
  private readonly auth = inject(AuthService);
  private readonly permissions = inject(PermissionsService);

  protected readonly status = signal<InvitePageStatus>('loading');
  protected readonly preview = signal<InvitationPreview | null>(null);
  protected readonly errorMessage = signal('');
  protected readonly recoveryAction = signal<InviteRecoveryAction>(null);
  readonly form = new FormGroup({
    displayName: new FormControl('', {
      nonNullable: true,
      validators: [Validators.required, Validators.maxLength(120)],
    }),
    password: new FormControl('', {
      nonNullable: true,
      validators: [Validators.required, Validators.minLength(8)],
    }),
  });
  protected readonly isAuthenticated = computed(
    () => this.currentUserService.currentUser() !== null,
  );
  protected readonly signedInEmailMismatch = computed(() => {
    const currentEmail = this.currentUserService.currentUser()?.email?.trim().toLowerCase();
    const invitedEmail = this.preview()?.email?.trim().toLowerCase();

    return Boolean(
      this.isAuthenticated() && currentEmail && invitedEmail && currentEmail !== invitedEmail,
    );
  });
  protected readonly cardSubtitle = computed(() => {
    const tenantName = this.preview()?.tenantName ?? 'your workspace';
    return `Join ${tenantName} on Customer Service Platform`;
  });

  private readonly token = this.route.snapshot.paramMap.get('token') ?? '';

  private readonly loadPreview = rxMethod<string>(
    pipe(
      tap(() => this.status.set('loading')),
      switchMap((token) =>
        this.api.previewInvitation(token).pipe(
          tap({
            next: (response) => {
              this.preview.set(response.data);
              this.recoveryAction.set(null);
              this.status.set(response.data.accountExists ? 'preview' : 'form');
            },
            error: (err) => {
              this.status.set('error');
              this.recoveryAction.set(this.recoveryActionForError(err));
              const code = err?.code ?? '';
              const status = err?.status ?? 0;
              if (code === 'NOT_FOUND' || status === 404) {
                this.errorMessage.set('This invitation link is invalid or has expired.');
              } else if (code === 'INVITATION_EXPIRED' || status === 410) {
                this.errorMessage.set(
                  'This invitation has expired. Ask a workspace admin to issue a fresh invitation.',
                );
              } else if (code === 'INVITATION_ACCEPTED' || status === 409) {
                this.errorMessage.set(
                  'This invitation has already been accepted. Please sign in to access the team.',
                );
              } else {
                this.errorMessage.set(err?.message ?? 'Failed to load invitation');
              }
            },
          }),
          catchError(() => EMPTY),
        ),
      ),
    ),
  );

  private readonly acceptInvite = rxMethod<AcceptInvitationRequest>(
    pipe(
      tap(() => {
        this.status.set('submitting');
        this.errorMessage.set('');
        this.clearControlErrors();
      }),
      switchMap((payload) => {
        // Snapshot the tenants the caller belonged to *before* this accept call, so the
        // success handler can tell which membership in the response is the one they just
        // joined — the previously active tenant (if any) is not necessarily the newly
        // joined one (e.g. an anonymous signup has none yet; a signed-in user may be
        // accepting an invite into a tenant other than the one currently active).
        const previousTenantIds = new Set(
          (this.currentUserService.currentUser()?.memberships ?? []).map((m) => m.tenantId),
        );
        return this.api.acceptInvitation(this.token, payload).pipe(
          switchMap((response) => {
            const memberships = response.data.memberships;
            const joinedMembership =
              memberships.find((m) => !previousTenantIds.has(m.tenantId)) ??
              memberships[memberships.length - 1] ??
              null;

            return from(this.currentUserService.load(joinedMembership?.tenantId)).pipe(
              map(() => response),
            );
          }),
          tap({
            next: () => {
              this.recoveryAction.set(null);
              this.status.set('success');
              void this.router.navigateByUrl(this.resolveLandingPath());
            },
            error: (err) => {
              this.recoveryAction.set(this.recoveryActionForError(err));
              this.errorMessage.set(this.mapAcceptError(err));
              this.status.set(this.isAuthenticated() ? 'error' : 'form');
            },
          }),
          catchError(() => EMPTY),
        );
      }),
    ),
  );

  constructor() {
    this.loadPreview(this.token);
  }

  protected submit(): void {
    if (this.form.invalid) {
      this.form.markAllAsTouched();
      return;
    }

    const { displayName, password } = this.form.getRawValue();
    this.acceptInvite({ displayName, password });
  }

  protected acceptSignedIn(): void {
    this.acceptInvite({});
  }

  protected goToLogin(): void {
    void this.router.navigate(['/', APP_PATHS.auth.base, APP_PATHS.auth.login], {
      queryParams: { returnUrl: this.inviteReturnUrl() },
    });
  }

  protected signOut(): void {
    void this.auth.logout({ returnUrl: this.inviteReturnUrl() });
  }

  protected controlError(name: 'displayName' | 'password'): string | null {
    const control = this.form.controls[name];
    if (!control.touched || !control.errors) return null;

    if (control.errors['required']) {
      return name === 'displayName' ? 'Display name is required.' : 'Password is required.';
    }
    if (name === 'displayName' && control.errors['maxlength']) {
      return 'Display name must be 120 characters or fewer.';
    }
    if (name === 'password' && control.errors['minlength']) {
      return 'Password must be at least 8 characters.';
    }
    if (control.errors['api']) {
      return control.errors['api'] as string;
    }

    return null;
  }

  private mapAcceptError(
    err: {
      status?: number;
      code?: string;
      message?: string;
      details?: { field?: string; message: string }[];
    } | null,
  ): string {
    const status = err?.status ?? 0;
    const code = err?.code ?? '';
    if (status === 403) {
      return 'This invitation was issued to a different email address.';
    }
    if (status === 409) {
      if ((err?.message ?? '').toLowerCase().includes('disabled')) {
        return 'Your account is disabled in this tenant.';
      }
      return 'You are already a member of this tenant.';
    }
    if (status === 410 || code === 'INVITATION_EXPIRED') {
      return 'This invitation has expired. Ask a workspace admin to issue a fresh invitation.';
    }
    if (status === 404 || code === 'NOT_FOUND') {
      return 'Invitation not found.';
    }
    if (status === 422) {
      this.applyFieldErrors(err);
      return err?.message ?? 'Please correct the highlighted fields.';
    }
    return err?.message ?? 'Failed to accept invitation';
  }

  private recoveryActionForError(
    err: { status?: number; code?: string; message?: string } | null,
  ): InviteRecoveryAction {
    const status = err?.status ?? 0;
    const code = err?.code ?? '';
    if (status === 403) {
      return 'switchAccount';
    }
    if (status === 409 && (err?.message ?? '').toLowerCase().includes('disabled')) {
      return 'reenableMembership';
    }
    if (status === 404 || code === 'NOT_FOUND' || status === 410 || code === 'INVITATION_EXPIRED') {
      return 'freshInvitation';
    }
    if (status === 409 || code === 'INVITATION_ACCEPTED') {
      return 'login';
    }
    return null;
  }

  private inviteReturnUrl(): string {
    return `/${APP_PATHS.invite}/${this.token}`;
  }

  private applyFieldErrors(err: { details?: { field?: string; message: string }[] } | null): void {
    for (const detail of err?.details ?? []) {
      const field = detail.field === 'display_name' ? 'displayName' : detail.field;
      if (field !== 'displayName' && field !== 'password') continue;

      const control = this.form.controls[field];
      control.setErrors({ ...(control.errors ?? {}), api: detail.message });
      control.markAsTouched();
    }
  }

  private clearControlErrors(): void {
    for (const control of [this.form.controls.displayName, this.form.controls.password]) {
      if (!control.errors) continue;
      const errors = { ...control.errors };
      delete errors['api'];
      control.setErrors(Object.keys(errors).length > 0 ? errors : null);
    }
  }

  private resolveLandingPath(): string {
    if (this.permissions.has('members.view')) {
      return `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.team}`;
    }

    for (const page of TENANT_LANDING_ORDER) {
      const permission = PAGE_PERMISSIONS[page];
      if (permission && this.permissions.has(permission)) {
        return `/${APP_PATHS.tenant.base}/${page}`;
      }
    }

    return `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.select}`;
  }
}
