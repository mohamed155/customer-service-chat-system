import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { FormControl, FormGroup, ReactiveFormsModule, Validators } from '@angular/forms';
import { ActivatedRoute, Router } from '@angular/router';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { catchError, EMPTY, from, map, pipe, switchMap, tap } from 'rxjs';
import { LocaleDatePipe } from '../../../core/i18n/format';
import { fieldErrorKey } from '../../../core/errors/error-message';
import { marker } from '../../../core/i18n/marker';
import { AcceptInvitationRequest, InvitationPreview } from '../../../core/api/tenant-api.models';
import { APP_PATHS } from '../../../core/router/app-paths';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { AuthService } from '../../../core/auth/auth.service';
import { PAGE_PERMISSIONS } from '../../../core/authz/permissions';
import { AuthCardComponent } from '../auth-card/auth-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { InvitationApiService } from './invitation-api.service';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

/** The part of an `ApiError` the page reads. */
interface InviteApiError {
  readonly requestId?: string;
  readonly status?: number;
  readonly code?: string;
  readonly details?: readonly { field?: string; code: string }[];
}

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
  providers: [provideTranslocoScope('auth')],
  imports: [
    LocaleDatePipe,
    ReactiveFormsModule,
    AuthCardComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    ErrorReferenceComponent,
    ButtonComponent,
    DashboardCardComponent,
    FormFieldComponent,
    InlineAlertComponent,
    TranslocoPipe,
  ],
  templateUrl: './accept-invitation.component.html',
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
  private readonly api = inject(InvitationApiService);
  private readonly currentUserService = inject(CurrentUserService);
  private readonly auth = inject(AuthService);
  private readonly permissions = inject(PermissionsService);

  protected readonly status = signal<InvitePageStatus>('loading');
  protected readonly preview = signal<InvitationPreview | null>(null);
  /** Catalog key of what went wrong, or `null`. */
  protected readonly errorKey = signal<string | null>(null);
  /** The request reference of the failed call, shown with the message (MOH-64). */
  protected readonly errorRequestId = signal<string | null>(null);
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
    const tenant = this.preview()?.tenantName;
    return tenant
      ? { key: marker('auth.invite.subtitle'), params: { tenant } }
      : { key: marker('auth.invite.subtitleAnonymous'), params: undefined };
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
              this.errorRequestId.set(err?.requestId ?? null);
              this.recoveryAction.set(this.recoveryActionForError(err));
              const status = err?.status ?? 0;
              if (status === 404) {
                this.errorKey.set(marker('auth.invite.errors.invalidLink'));
              } else if (status === 410) {
                this.errorKey.set(marker('auth.invite.errors.expired'));
              } else if (status === 409) {
                this.errorKey.set(marker('auth.invite.errors.alreadyAccepted'));
              } else {
                this.errorKey.set(marker('auth.invite.errors.loadFailed'));
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
        this.errorKey.set(null);
        this.errorRequestId.set(null);
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
              this.errorRequestId.set(err?.requestId ?? null);
              this.errorKey.set(this.mapAcceptError(err));
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
      return name === 'displayName'
        ? marker('auth.invite.errors.displayNameRequired')
        : marker('auth.invite.errors.passwordRequired');
    }
    if (name === 'displayName' && control.errors['maxlength']) {
      return marker('auth.invite.errors.displayNameTooLong');
    }
    if (name === 'password' && control.errors['minlength']) {
      return marker('auth.invite.errors.passwordShort');
    }
    // The key of an error the API reported for this field.
    if (control.errors['api']) {
      return control.errors['api'] as string;
    }

    return null;
  }

  /** The catalog key of what to tell the person; the API's own text is never shown. */
  private mapAcceptError(err: InviteApiError | null): string {
    const status = err?.status ?? 0;
    if (status === 403) {
      return marker('auth.invite.errors.differentEmail');
    }
    if (status === 409) {
      return err?.code === 'membership_disabled'
        ? marker('auth.invite.errors.disabled')
        : marker('auth.invite.errors.alreadyMember');
    }
    if (status === 410) {
      return marker('auth.invite.errors.expired');
    }
    if (status === 404) {
      return marker('auth.invite.errors.notFound');
    }
    if (status === 422) {
      this.applyFieldErrors(err);
      return marker('auth.invite.errors.fixFields');
    }
    return marker('auth.invite.errors.acceptFailed');
  }

  private recoveryActionForError(err: InviteApiError | null): InviteRecoveryAction {
    const status = err?.status ?? 0;
    if (status === 403) {
      return 'switchAccount';
    }
    if (status === 409 && err?.code === 'membership_disabled') {
      return 'reenableMembership';
    }
    if (status === 404 || status === 410) {
      return 'freshInvitation';
    }
    if (status === 409) {
      return 'login';
    }
    return null;
  }

  private inviteReturnUrl(): string {
    return `/${APP_PATHS.invite}/${this.token}`;
  }

  private applyFieldErrors(err: InviteApiError | null): void {
    for (const detail of err?.details ?? []) {
      const field = detail.field === 'display_name' ? 'displayName' : detail.field;
      if (field !== 'displayName' && field !== 'password') continue;

      const control = this.form.controls[field];
      control.setErrors({ ...(control.errors ?? {}), api: fieldErrorKey(detail.code) });
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
