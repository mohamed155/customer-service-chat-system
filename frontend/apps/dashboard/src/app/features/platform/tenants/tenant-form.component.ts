import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
} from '@angular/core';
import { FormBuilder, ReactiveFormsModule, Validators } from '@angular/forms';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { catchError, EMPTY, pipe, Subject, switchMap, tap } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import {
  CreateTenantPayload,
  PlatformTenantDetail,
  TenantPlan,
  UpdateTenantPayload,
} from '../../../core/api/tenant-api.models';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { APP_PATHS } from '../../../core/router/app-paths';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PlatformTenantsService } from './platform-tenants.service';
import { TenantsStore } from './tenants.store';

const SLUG_PATTERN = /^[a-z0-9](-?[a-z0-9])*$/;
const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

@Component({
  selector: 'app-tenant-form',
  imports: [
    ReactiveFormsModule,
    RouterLink,
    PageContainerComponent,
    PageHeaderComponent,
    EmptyStateComponent,
    LoadingStateComponent,
  ],
  template: `
    <app-page-container>
      <app-page-header
        [title]="isEditMode() ? 'Edit tenant' : 'New tenant'"
        [description]="
          isEditMode() ? 'Update this customer organization' : 'Onboard a new customer organization'
        "
      />

      @if (!canManage()) {
        <app-empty-state
          icon="@tui.shield-off"
          title="Management not permitted"
          description="You do not have permission to manage tenants. Please contact an administrator if you need access."
        />
      } @else if (loadingInitial()) {
        <app-loading-state label="Loading tenant" />
      } @else if (editInitFailed()) {
        <app-empty-state
          icon="@tui.alert-circle"
          title="Couldn't load tenant"
          description="We couldn't load this tenant for editing. Please try again."
        >
          <button type="button" class="primary-button" (click)="retryLoad()">Try again</button>
        </app-empty-state>
      } @else {
        <form [formGroup]="form" (ngSubmit)="onSubmit()" class="form" novalidate>
          <label class="field">
            <span>Display name *</span>
            <input
              type="text"
              formControlName="name"
              maxlength="200"
              autocomplete="off"
              [attr.aria-invalid]="ariaInvalid('name') ? 'true' : null"
              [attr.aria-describedby]="ariaInvalid('name') ? errorIdFor('name') : null"
            />
            @if (showError('name')) {
              <small [id]="errorIdFor('name')" class="error">{{ errorMessage('name') }}</small>
            }
          </label>

          <label class="field">
            <span>Slug *</span>
            <input
              type="text"
              formControlName="slug"
              maxlength="63"
              autocomplete="off"
              [attr.aria-invalid]="ariaInvalid('slug') ? 'true' : null"
              [attr.aria-describedby]="ariaInvalid('slug') ? errorIdFor('slug') : null"
            />
            <small class="hint">Lowercase letters, digits, and single hyphens. Max 63 chars.</small>
            @if (showError('slug')) {
              <small [id]="errorIdFor('slug')" class="error">{{ errorMessage('slug') }}</small>
            }
          </label>

          <label class="field">
            <span>Plan</span>
            <select
              formControlName="plan"
              [attr.aria-invalid]="ariaInvalid('plan') ? 'true' : null"
              [attr.aria-describedby]="ariaInvalid('plan') ? errorIdFor('plan') : null"
            >
              <option value="trial">Trial</option>
              <option value="starter">Starter</option>
              <option value="professional">Professional</option>
              <option value="enterprise">Enterprise</option>
            </select>
            @if (showError('plan')) {
              <small [id]="errorIdFor('plan')" class="error">{{ errorMessage('plan') }}</small>
            }
          </label>

          <label class="field">
            <span>Contact name</span>
            <input
              type="text"
              formControlName="contactName"
              maxlength="200"
              autocomplete="off"
              [attr.aria-invalid]="ariaInvalid('contactName') ? 'true' : null"
              [attr.aria-describedby]="
                ariaInvalid('contactName') ? errorIdFor('contactName') : null
              "
            />
            @if (showError('contactName')) {
              <small [id]="errorIdFor('contactName')" class="error">{{
                errorMessage('contactName')
              }}</small>
            }
          </label>

          <label class="field">
            <span>Contact email</span>
            <input
              type="email"
              formControlName="contactEmail"
              autocomplete="off"
              [attr.aria-invalid]="ariaInvalid('contactEmail') ? 'true' : null"
              [attr.aria-describedby]="
                ariaInvalid('contactEmail') ? errorIdFor('contactEmail') : null
              "
            />
            @if (showError('contactEmail')) {
              <small [id]="errorIdFor('contactEmail')" class="error">{{
                errorMessage('contactEmail')
              }}</small>
            }
          </label>

          @if (serverError(); as err) {
            <div class="form-error" role="alert">{{ err.message }}</div>
          }

          <div class="actions">
            <a [routerLink]="cancelLink()">Cancel</a>
            <button type="submit" [disabled]="submitting() || form.invalid || loadingInitial()">
              {{ submitting() ? (isEditMode() ? 'Saving…' : 'Creating…') : submitLabel() }}
            </button>
          </div>
        </form>
      }

      @if (submitting()) {
        <app-loading-state [label]="isEditMode() ? 'Saving changes' : 'Creating tenant'" />
      }
    </app-page-container>
  `,
  styles: [
    `
      .form {
        max-width: 560px;
        display: grid;
        gap: var(--app-space-4);
      }
      .field {
        display: grid;
        gap: var(--app-space-1);
      }
      .field > span {
        font-weight: 600;
        font-size: var(--app-font-sm);
        color: var(--app-text);
      }
      .field input,
      .field select {
        height: 38px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        font: inherit;
      }
      .field input:focus,
      .field select:focus {
        outline: 0;
        border-color: var(--app-accent);
        box-shadow: 0 0 0 3px var(--app-accent-soft);
      }
      .hint {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .error {
        color: var(--app-red);
        font-size: var(--app-font-xs);
      }
      .form-error {
        padding: var(--app-space-3);
        border: 1px solid var(--app-red);
        border-radius: var(--app-radius-md);
        background: color-mix(in srgb, var(--app-red) 10%, var(--app-panel));
        color: var(--app-red);
        font-size: var(--app-font-sm);
      }
      .actions {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: var(--app-space-3);
      }
      .actions a {
        color: var(--app-text-2);
        text-decoration: none;
        font: inherit;
      }
      .actions a:hover {
        color: var(--app-text);
        text-decoration: underline;
      }
      .actions button {
        height: 38px;
        padding: 0 var(--app-space-4);
        border: 0;
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-on, white);
        font: inherit;
        font-weight: 600;
        cursor: pointer;
      }
      .actions button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
      .actions button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      .primary-button {
        height: 38px;
        padding: 0 var(--app-space-4);
        border: 0;
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-on, white);
        font: inherit;
        font-weight: 600;
        cursor: pointer;
      }
      .primary-button:hover {
        opacity: 0.92;
      }
      .primary-button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TenantFormComponent {
  private readonly fb = inject(FormBuilder);
  private readonly store = inject(TenantsStore);
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);
  private readonly service = inject(PlatformTenantsService);
  private readonly permissions = inject(PermissionsService);

  private readonly loadEdit$ = new Subject<string>();
  private readonly loadEdit = rxMethod<string>(
    pipe(
      tap(() => {
        this.loadingInitial.set(true);
        this.serverError.set(null);
      }),
      switchMap((id) =>
        this.service.get(id).pipe(
          tap((response) => {
            if (this.initial() === null) {
              this.fetchedInitial.set(response.data);
              this.applyDetail(response.data);
            }
            this.loadingInitial.set(false);
          }),
          catchError((err: ApiError) => {
            this.serverError.set(err);
            this.loadingInitial.set(false);
            return EMPTY;
          }),
        ),
      ),
    ),
  );

  private readonly create$ = new Subject<CreateTenantPayload>();
  private readonly createAction = rxMethod<CreateTenantPayload>(
    pipe(
      tap(() => {
        this.submitting.set(true);
        this.serverError.set(null);
      }),
      switchMap((payload) =>
        this.store.create(payload).pipe(
          tap(() => {
            this.submitting.set(false);
            void this.router.navigate(['/', APP_PATHS.platform.base, APP_PATHS.platform.tenants]);
          }),
          catchError((apiError: ApiError) => {
            this.serverError.set(apiError);
            this.applyServerFieldErrors(apiError);
            this.submitting.set(false);
            return EMPTY;
          }),
        ),
      ),
    ),
  );

  private readonly update$ = new Subject<{ id: string; payload: UpdateTenantPayload }>();
  private readonly updateAction = rxMethod<{ id: string; payload: UpdateTenantPayload }>(
    pipe(
      tap(() => {
        this.submitting.set(true);
        this.serverError.set(null);
      }),
      switchMap(({ id, payload }) =>
        this.store.update(id, payload).pipe(
          tap(() => {
            this.submitting.set(false);
            void this.router.navigate([
              '/',
              APP_PATHS.platform.base,
              APP_PATHS.platform.tenants,
              id,
            ]);
          }),
          catchError((apiError: ApiError) => {
            this.serverError.set(apiError);
            this.applyServerFieldErrors(apiError);
            this.submitting.set(false);
            return EMPTY;
          }),
        ),
      ),
    ),
  );

  protected readonly APP_PATHS = APP_PATHS;
  protected readonly tenantId = signal<string | null>(null);
  readonly initial = input<PlatformTenantDetail | null>(null);
  protected readonly fetchedInitial = signal<PlatformTenantDetail | null>(null);
  protected readonly loadingInitial = signal(false);
  protected readonly canManage = computed(() => this.permissions.has('platform.tenants.manage'));
  protected readonly editingId = computed<string | null>(() => {
    const initial = this.initial();
    if (initial !== null) {
      return initial.id;
    }
    return this.tenantId();
  });
  protected readonly isEditMode = computed(() => this.editingId() !== null);
  protected readonly editInitFailed = computed(
    () =>
      this.isEditMode() &&
      !this.loadingInitial() &&
      this.initial() === null &&
      this.fetchedInitial() === null &&
      this.serverError() !== null,
  );

  protected readonly submitting = signal(false);
  protected readonly serverError = signal<ApiError | null>(null);

  readonly form = this.fb.nonNullable.group({
    name: ['', [Validators.required, Validators.minLength(1), Validators.maxLength(200)]],
    slug: ['', [Validators.required, Validators.pattern(SLUG_PATTERN), Validators.maxLength(63)]],
    plan: ['trial' as TenantPlan, Validators.required],
    contactName: ['', [Validators.maxLength(200)]],
    contactEmail: ['', [Validators.pattern(EMAIL_PATTERN)]],
  });

  protected readonly submitLabel = computed(() =>
    this.isEditMode() ? 'Save changes' : 'Create tenant',
  );

  protected readonly cancelLink = computed(() => {
    const id = this.editingId();
    if (id) {
      return ['/', APP_PATHS.platform.base, APP_PATHS.platform.tenants, id];
    }
    return ['/', APP_PATHS.platform.base, APP_PATHS.platform.tenants];
  });

  constructor() {
    this.loadEdit(this.loadEdit$);
    this.createAction(this.create$);
    this.updateAction(this.update$);

    const id = this.route.snapshot.paramMap.get('id');
    if (id) {
      this.tenantId.set(id);
    }

    effect(() => {
      if (!this.canManage()) {
        return;
      }
      const initial = this.initial();
      if (initial) {
        this.applyDetail(initial);
        return;
      }
      const routeId = this.tenantId();
      if (
        routeId &&
        !this.fetchedInitial() &&
        !this.loadingInitial() &&
        this.serverError() === null
      ) {
        this.fetchById(routeId);
      }
    });
  }

  private fetchById(id: string): void {
    this.loadEdit$.next(id);
  }

  private applyDetail(detail: PlatformTenantDetail): void {
    this.form.patchValue({
      name: detail.name,
      slug: detail.slug,
      plan: detail.plan,
      contactName: detail.contactName ?? '',
      contactEmail: detail.contactEmail ?? '',
    });
  }

  protected retryLoad(): void {
    const id = this.editingId();
    if (!id) return;
    this.fetchById(id);
  }

  protected showError(field: keyof typeof this.form.controls): boolean {
    const control = this.form.controls[field];
    return control.invalid && (control.touched || control.dirty);
  }

  protected ariaInvalid(field: keyof typeof this.form.controls): boolean {
    const control = this.form.controls[field];
    return control.invalid && (control.touched || control.dirty);
  }

  protected errorIdFor(field: string): string {
    return `tenant-form-${field}-error`;
  }

  protected errorMessage(field: keyof typeof this.form.controls): string {
    const control = this.form.controls[field];
    if (control.hasError('server')) {
      return control.getError('server') as string;
    }
    if (control.hasError('required')) return 'This field is required';
    if (control.hasError('pattern')) {
      if (field === 'slug')
        return 'Slug must be lowercase letters/digits with optional single hyphens';
      if (field === 'contactEmail') return 'Enter a valid email address';
      return 'Invalid format';
    }
    if (control.hasError('maxlength')) return 'Value is too long';
    if (control.hasError('minlength')) return 'Value is too short';
    return 'Invalid value';
  }

  protected onSubmit(): void {
    if (this.loadingInitial() || this.form.invalid || this.submitting()) {
      this.form.markAllAsTouched();
      return;
    }
    const value = this.form.getRawValue();
    const editingId = this.editingId();

    if (editingId) {
      const payload: UpdateTenantPayload = {
        name: value.name,
        slug: value.slug,
        plan: value.plan,
        contactName: value.contactName === '' ? null : value.contactName,
        contactEmail: value.contactEmail === '' ? null : value.contactEmail,
      };
      this.update$.next({ id: editingId, payload });
    } else {
      const payload: CreateTenantPayload = {
        name: value.name,
        slug: value.slug,
        plan: value.plan,
        contactName: value.contactName || undefined,
        contactEmail: value.contactEmail || undefined,
      };
      this.create$.next(payload);
    }
  }

  private applyServerFieldErrors(apiError: ApiError): void {
    const fieldMap: Record<string, keyof typeof this.form.controls> = {
      name: 'name',
      slug: 'slug',
      plan: 'plan',
      contactName: 'contactName',
      contactEmail: 'contactEmail',
    };
    let slugAlreadyAttached = false;
    if (apiError.details) {
      for (const detail of apiError.details) {
        if (!detail.field) continue;
        const key = fieldMap[detail.field] ?? (detail.field as keyof typeof this.form.controls);
        if (this.form.controls[key]) {
          this.form.controls[key].setErrors({ server: detail.message });
          this.form.controls[key].markAsTouched();
          if (key === 'slug') slugAlreadyAttached = true;
        }
      }
    }
    if (!slugAlreadyAttached && apiError.code === 'conflict' && apiError.status === 409) {
      this.form.controls.slug.setErrors({ server: apiError.message });
      this.form.controls.slug.markAsTouched();
    }
  }
}
