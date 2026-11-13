import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { RouterLink } from '@angular/router';
import { Subscription, catchError, EMPTY, tap } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { CreateCustomerPayload } from '../../../core/api/tenant-api.models';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { onTenantChange } from '../../../core/state/tenant-scope';
import { APP_PATHS } from '../../../core/router/app-paths';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { ChannelBadgeComponent } from '../../../shared/components/channel-badge/channel-badge.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { SearchInputComponent } from '../../../shared/components/search-input/search-input.component';
import { ToolbarComponent } from '../../../shared/components/toolbar/toolbar.component';
import { CustomersApiService } from '../../../core/api/customers-api.service';
import { CustomerDialogComponent } from './customer-dialog.component';
import { CustomersStore } from './customers.store';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { errorMessageKey } from '../../../core/errors/error-message';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

@Component({
  selector: 'app-customers',
  providers: [provideTranslocoScope('customers')],
  imports: [
    ErrorReferenceComponent,
    RouterLink,
    AvatarComponent,
    ButtonComponent,
    ChannelBadgeComponent,
    CustomerDialogComponent,
    DataTableComponent,
    EmptyStateComponent,
    InlineAlertComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SearchInputComponent,
    ToolbarComponent,
    TranslocoPipe,
  ],
  templateUrl: './customers.component.html',
  styles: [
    `
      .stack {
        display: grid;
        gap: var(--app-space-4);
      }
      .cell-name {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
      }
      .name-link {
        color: var(--app-text);
        text-decoration: none;
        font-weight: 600;
      }
      .name-link:hover {
        text-decoration: underline;
      }
      .contact-cell {
        display: flex;
        flex-direction: column;
        gap: 2px;
      }
      .contact-cell .muted {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .channels-cell {
        display: flex;
        flex-wrap: wrap;
        gap: var(--app-space-2);
      }
      .load-more {
        display: flex;
        justify-content: center;
        padding: var(--app-space-3);
      }
      caption {
        position: absolute;
        width: 1px;
        height: 1px;
        overflow: hidden;
        clip: rect(0 0 0 0);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CustomersComponent {
  protected readonly store = inject(CustomersStore);
  private readonly api = inject(CustomersApiService);
  private readonly permissions = inject(PermissionsService);
  private readonly destroyRef = inject(DestroyRef);
  private createSubscription: Subscription | null = null;

  protected readonly showLoading = computed(() => {
    const status = this.store.status();
    return status === 'pending' || (status === 'loading' && this.store.items().length === 0);
  });

  protected readonly shouldShowDirectory = computed(() => {
    if (this.showLoading()) return false;
    return this.store.status() !== 'error';
  });

  protected readonly isLoadingMore = computed(
    () =>
      this.store.status() === 'loading' && this.store.hasMore() && this.store.items().length > 0,
  );

  protected readonly canManage = computed(() => this.permissions.has('customers.manage'));

  /** Catalog key of the text for an error of this page. */
  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  constructor() {
    onTenantChange(() => {
      this.createSubscription?.unsubscribe();
      this.createSubscription = null;
      this.closeCreateDialog();
    });

    effect(() => {
      if (this.showCreateDialog() && !this.canManage()) {
        this.createSubscription?.unsubscribe();
        this.createSubscription = null;
        this.closeCreateDialog();
      }
    });
  }

  protected readonly showCreateDialog = signal(false);
  protected readonly dialogSubmitting = signal(false);
  protected readonly dialogError = signal<ApiError | null>(null);

  protected openCreateDialog(): void {
    this.showCreateDialog.set(true);
    this.dialogError.set(null);
  }

  protected closeCreateDialog(): void {
    this.showCreateDialog.set(false);
    this.dialogSubmitting.set(false);
    this.dialogError.set(null);
  }

  protected onCreate(payload: CreateCustomerPayload): void {
    this.createSubscription?.unsubscribe();
    this.dialogSubmitting.set(true);
    this.dialogError.set(null);
    this.createSubscription = this.api
      .createCustomer(payload)
      .pipe(
        takeUntilDestroyed(this.destroyRef),
        tap(() => {
          this.closeCreateDialog();
          this.store.load();
        }),
        catchError((error: unknown) => {
          this.dialogSubmitting.set(false);
          this.dialogError.set(error as ApiError);
          return EMPTY;
        }),
      )
      .subscribe();
  }

  protected initials(displayName: string): string {
    return displayName
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((part) => part[0] ?? '')
      .join('')
      .toUpperCase();
  }

  protected profileLink(id: string): string {
    return `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.customerDetail(id)}`;
  }

  protected onSearchChange(query: string): void {
    this.store.search(query);
  }
}
