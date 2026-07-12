import { DatePipe } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { catchError, concatMap, EMPTY, pipe, Subject, switchMap, tap } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { PlatformTenantDetail, UpdateTenantPayload } from '../../../core/api/tenant-api.models';
import { HasPermissionDirective } from '../../../core/authz/has-permission.directive';
import { APP_PATHS } from '../../../core/router/app-paths';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { TenantsStore } from './tenants.store';

@Component({
  selector: 'app-tenant-detail',
  imports: [
    DatePipe,
    RouterLink,
    PageContainerComponent,
    PageHeaderComponent,
    LoadingStateComponent,
    EmptyStateComponent,
    StatusBadgeComponent,
    HasPermissionDirective,
  ],
  template: `
    <app-page-container>
      <app-page-header title="Tenant details" description="View this organization's record">
        <div class="header-actions">
          <a
            [routerLink]="['/', APP_PATHS.platform.base, APP_PATHS.platform.tenants]"
            class="back-link"
          >
            ← Back to tenants
          </a>
          @if (tenant(); as t) {
            <a
              *appHasPermission="'platform.tenants.manage'"
              [routerLink]="[
                '/',
                APP_PATHS.platform.base,
                APP_PATHS.platform.tenants,
                t.id,
                'edit',
              ]"
              class="action-link"
            >
              Edit
            </a>
            <button
              *appHasPermission="'platform.tenants.manage'"
              type="button"
              class="action-button"
              [class.danger]="t.status === 'active'"
              [disabled]="actionPending()"
              (click)="onToggleStatus(t)"
            >
              {{ t.status === 'active' ? 'Deactivate' : 'Reactivate' }}
            </button>
          }
        </div>
        @if (actionError(); as errMsg) {
          <div class="action-error" role="alert">{{ errMsg }}</div>
        }
      </app-page-header>

      @if (loading()) {
        <app-loading-state />
      } @else if (error(); as err) {
        <div role="alert">
          <app-empty-state
            icon="@tui.alert-circle"
            [title]="err.message"
            description="We couldn't load this tenant. Please try again."
          >
            <button type="button" class="primary-button" (click)="load()">Try again</button>
          </app-empty-state>
        </div>
      } @else if (tenant(); as t) {
        <dl class="record">
          <div class="row">
            <dt>Name</dt>
            <dd>{{ t.name }}</dd>
          </div>
          <div class="row">
            <dt>Slug</dt>
            <dd class="muted">{{ t.slug }}</dd>
          </div>
          <div class="row">
            <dt>Status</dt>
            <dd>
              <app-status-badge
                [status]="t.status"
                [tone]="t.status === 'active' ? 'green' : 'neutral'"
              />
            </dd>
          </div>
          <div class="row">
            <dt>Plan</dt>
            <dd class="muted">{{ planLabel() }}</dd>
          </div>
          <div class="row">
            <dt>Contact name</dt>
            <dd>{{ t.contactName ?? '—' }}</dd>
          </div>
          <div class="row">
            <dt>Contact email</dt>
            <dd>{{ t.contactEmail ?? '—' }}</dd>
          </div>
          <div class="row">
            <dt>Created</dt>
            <dd class="muted">{{ t.createdAt | date: 'medium' }}</dd>
          </div>
          <div class="row">
            <dt>Updated</dt>
            <dd class="muted">{{ t.updatedAt | date: 'medium' }}</dd>
          </div>
        </dl>
      }

      @if (showConfirmDialog()) {
        <div
          class="dialog-backdrop"
          (click)="cancelDialog()"
          (keydown.enter)="cancelDialog()"
          (keydown.space)="cancelDialog(); $event.preventDefault()"
          tabindex="0"
          role="presentation"
        ></div>
        <div
          #dialogContainer
          class="dialog"
          role="alertdialog"
          aria-labelledby="dialog-title"
          aria-describedby="dialog-desc"
          (keydown.escape)="cancelDialog()"
          (keydown)="onDialogKeydown($event)"
        >
          <h2 id="dialog-title">{{ dialogTitle() }}</h2>
          <p id="dialog-desc">{{ dialogMessage() }}</p>
          <div class="dialog-actions">
            <button type="button" class="dialog-cancel" (click)="cancelDialog()">Cancel</button>
            <button type="button" class="dialog-confirm danger" (click)="confirmDialog()">
              {{ dialogConfirmLabel() }}
            </button>
          </div>
        </div>
      }
    </app-page-container>
  `,
  styles: [
    `
      .header-actions {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
      }
      .back-link {
        color: var(--app-accent);
        text-decoration: none;
        font-size: var(--app-font-sm);
        font-weight: 500;
      }
      .back-link:hover {
        text-decoration: underline;
      }
      .back-link:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-radius: var(--app-radius-xs);
      }
      .action-link {
        height: 38px;
        display: inline-flex;
        align-items: center;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        text-decoration: none;
        font: inherit;
        font-size: var(--app-font-sm);
        font-weight: 600;
      }
      .action-link:hover {
        background: var(--app-panel-2);
      }
      .action-link:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      .action-button {
        height: 38px;
        padding: 0 var(--app-space-3);
        border: 0;
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-on, white);
        font: inherit;
        font-size: var(--app-font-sm);
        font-weight: 600;
        cursor: pointer;
      }
      .action-button.danger {
        background: var(--app-red, #d92d20);
      }
      .action-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
      .action-button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      .record {
        max-width: 640px;
        display: grid;
        gap: var(--app-space-3);
        padding: var(--app-space-4);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
      }
      .row {
        display: grid;
        grid-template-columns: 160px 1fr;
        align-items: center;
        gap: var(--app-space-3);
      }
      dt {
        font-weight: 600;
        font-size: var(--app-font-sm);
        color: var(--app-text-2);
      }
      dd {
        margin: 0;
        color: var(--app-text);
        font-size: var(--app-font-sm);
      }
      .muted {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
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
      .action-error {
        margin-top: var(--app-space-3);
        padding: var(--app-space-2) var(--app-space-3);
        background: var(--app-red-bg, #fef3f2);
        border: 1px solid var(--app-red, #d92d20);
        border-radius: var(--app-radius-md);
        color: var(--app-red, #d92d20);
        font-size: var(--app-font-sm);
        font-weight: 500;
      }
      .dialog-backdrop {
        position: fixed;
        inset: 0;
        z-index: 999;
        background: rgba(0, 0, 0, 0.4);
      }
      .dialog {
        position: fixed;
        top: 50%;
        left: 50%;
        transform: translate(-50%, -50%);
        z-index: 1000;
        background: var(--app-panel);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        padding: var(--app-space-5);
        max-width: 440px;
        width: 90%;
        box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
      }
      .dialog h2 {
        margin: 0 0 var(--app-space-2);
        font-size: var(--app-font-lg);
        color: var(--app-text);
      }
      .dialog p {
        margin: 0 0 var(--app-space-4);
        font-size: var(--app-font-sm);
        color: var(--app-text-2);
        line-height: 1.5;
      }
      .dialog-actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--app-space-3);
      }
      .dialog-cancel,
      .dialog-confirm {
        height: 38px;
        padding: 0 var(--app-space-4);
        border-radius: var(--app-radius-md);
        font: inherit;
        font-weight: 600;
        cursor: pointer;
      }
      .dialog-cancel {
        background: var(--app-panel-2);
        border: 1px solid var(--app-border);
        color: var(--app-text);
      }
      .dialog-confirm {
        border: 0;
        background: var(--app-accent);
        color: var(--app-accent-on, white);
      }
      .dialog-confirm.danger {
        background: var(--app-red, #d92d20);
      }
      .dialog-confirm:focus-visible,
      .dialog-cancel:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TenantDetailComponent {
  private readonly route = inject(ActivatedRoute);
  private readonly store = inject(TenantsStore);

  protected readonly APP_PATHS = APP_PATHS;
  protected readonly tenant = signal<PlatformTenantDetail | null>(null);
  protected readonly loading = signal(true);
  protected readonly error = signal<ApiError | null>(null);
  protected readonly actionPending = signal(false);
  protected readonly actionError = signal<string | null>(null);

  protected readonly showConfirmDialog = signal(false);
  protected readonly pendingToggleTenant = signal<PlatformTenantDetail | null>(null);
  protected readonly dialogTitle = signal('');
  protected readonly dialogMessage = signal('');
  protected readonly dialogConfirmLabel = signal('');

  private readonly previouslyFocused = signal<HTMLElement | null>(null);

  protected readonly planLabel = computed(() => {
    const plan = this.tenant()?.plan;
    return plan ? plan.charAt(0).toUpperCase() + plan.slice(1) : '—';
  });

  private readonly loadDetail$ = new Subject<void>();
  private readonly loadDetail = rxMethod<void>(
    pipe(
      tap(() => {
        this.loading.set(true);
        this.error.set(null);
      }),
      switchMap(() => {
        const id = this.route.snapshot.paramMap.get('id');
        if (!id) {
          this.error.set({ code: 'not_found', message: 'No tenant id in route', status: 404 });
          this.loading.set(false);
          return EMPTY;
        }
        return this.store.getDetail(id).pipe(
          tap((detail) => {
            this.tenant.set(detail);
            this.loading.set(false);
          }),
          catchError((err: ApiError) => {
            this.error.set(err);
            this.loading.set(false);
            return EMPTY;
          }),
        );
      }),
    ),
  );

  private readonly toggleStatus$ = new Subject<PlatformTenantDetail>();
  private readonly toggleStatus = rxMethod<PlatformTenantDetail>(
    pipe(
      tap(() => {
        this.actionPending.set(true);
        this.actionError.set(null);
      }),
      concatMap((t) => {
        const newStatus: 'active' | 'suspended' = t.status === 'active' ? 'suspended' : 'active';
        const payload: UpdateTenantPayload = { status: newStatus };
        return this.store.update(t.id, payload).pipe(
          tap((updated) => {
            this.tenant.set(updated);
            this.actionPending.set(false);
          }),
          catchError((err: ApiError) => {
            this.actionError.set(err.message);
            this.actionPending.set(false);
            return EMPTY;
          }),
        );
      }),
    ),
  );

  constructor() {
    this.loadDetail(this.loadDetail$);
    this.toggleStatus(this.toggleStatus$);
    this.loadDetail$.next();

    effect(() => {
      if (this.showConfirmDialog()) {
        setTimeout(() => {
          const cancelBtn = document.querySelector<HTMLElement>('.dialog-cancel');
          cancelBtn?.focus();
        });
      }
    });
  }

  protected load(): void {
    this.loadDetail$.next();
  }

  protected onToggleStatus(t: PlatformTenantDetail): void {
    this.previouslyFocused.set(document.activeElement as HTMLElement);
    const newStatus = t.status === 'active' ? 'suspended' : 'active';
    this.pendingToggleTenant.set(t);
    if (newStatus === 'suspended') {
      this.dialogTitle.set('Deactivate tenant');
      this.dialogMessage.set(
        `Deactivating "${t.name}" will immediately block all of its members from accessing the workspace. Continue?`,
      );
      this.dialogConfirmLabel.set('Deactivate');
    } else {
      this.dialogTitle.set('Reactivate tenant');
      this.dialogMessage.set(`Reactivate "${t.name}" and restore member access?`);
      this.dialogConfirmLabel.set('Reactivate');
    }
    this.showConfirmDialog.set(true);
  }

  protected cancelDialog(): void {
    this.showConfirmDialog.set(false);
    this.pendingToggleTenant.set(null);
    this.previouslyFocused()?.focus();
  }

  protected confirmDialog(): void {
    const t = this.pendingToggleTenant();
    if (t) {
      this.toggleStatus$.next(t);
    }
    this.showConfirmDialog.set(false);
    this.pendingToggleTenant.set(null);
    this.previouslyFocused()?.focus();
  }

  protected onDialogKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Tab') return;
    const container = event.currentTarget as HTMLElement;
    const focusable = container.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      last.focus();
      event.preventDefault();
    } else if (!event.shiftKey && document.activeElement === last) {
      first.focus();
      event.preventDefault();
    }
  }
}
