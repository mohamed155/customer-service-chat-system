import { LocaleDatePipe } from '../../../core/i18n/format';
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
import { ActivatedRoute, RouterLink } from '@angular/router';
import { catchError, distinctUntilChanged, EMPTY, filter, map, tap } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { ConversationSummary, UpdateCustomerPayload } from '../../../core/api/tenant-api.models';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { onTenantChange } from '../../../core/state/tenant-scope';
import { APP_PATHS } from '../../../core/router/app-paths';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { ChannelBadgeComponent } from '../../../shared/components/channel-badge/channel-badge.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import {
  BadgeTone,
  StatusBadgeComponent,
} from '../../../shared/components/status-badge/status-badge.component';
import { CustomersApiService } from '../../../core/api/customers-api.service';
import { CustomerDialogComponent } from './customer-dialog.component';
import { CustomerProfileStore } from './customer-profile.store';
import { errorMessageKey } from '../../../core/errors/error-message';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

@Component({
  selector: 'app-customer-profile',
  providers: [provideTranslocoScope('customers')],
  imports: [
    LocaleDatePipe,
    RouterLink,
    AvatarComponent,
    ButtonComponent,
    ChannelBadgeComponent,
    CustomerDialogComponent,
    DashboardCardComponent,
    EmptyStateComponent,
    ErrorReferenceComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
    TranslocoPipe,
  ],
  templateUrl: './customer-profile.component.html',
  styles: [
    `
      .back-link {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-1);
        margin-bottom: var(--app-space-3);
        color: var(--app-text-2);
        text-decoration: none;
        font-size: var(--app-font-sm);
        font-weight: 600;
      }
      .back-link:hover {
        color: var(--app-accent);
      }
      .profile-grid {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: var(--app-space-4);
      }
      .profile-grid app-dashboard-card:nth-child(3),
      .profile-grid app-dashboard-card:nth-child(4) {
        grid-column: 1 / -1;
      }
      .contact {
        display: flex;
        align-items: center;
        gap: var(--app-space-4);
      }
      .contact-body {
        display: grid;
        gap: var(--app-space-2);
        min-width: 0;
      }
      .name {
        color: var(--app-text);
        font-size: var(--app-font-lg);
        font-weight: 650;
      }
      .contact-row {
        display: flex;
        gap: var(--app-space-3);
        align-items: baseline;
        font-size: var(--app-font-sm);
      }
      .label {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        min-width: 60px;
      }
      .timestamps {
        display: flex;
        flex-wrap: wrap;
        gap: var(--app-space-5);
        margin-top: var(--app-space-4);
        padding-top: var(--app-space-4);
        border-top: 1px solid var(--app-border);
      }
      .timestamps > div {
        display: grid;
        gap: 2px;
      }
      .timestamps time {
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 600;
      }
      .identifiers {
        list-style: none;
        margin: 0;
        padding: 0;
        display: grid;
        gap: var(--app-space-2);
      }
      .identifiers li {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        padding: var(--app-space-2) 0;
        border-bottom: 1px solid var(--app-border);
      }
      .identifiers li:last-child {
        border-bottom: 0;
      }
      .identifier-value {
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-family: var(--app-font-mono);
        word-break: break-all;
      }
      .metadata {
        margin: 0;
        display: grid;
        gap: var(--app-space-2);
      }
      .metadata-row {
        display: grid;
        grid-template-columns: minmax(0, 1fr) minmax(0, 2fr);
        gap: var(--app-space-3);
        padding: var(--app-space-2) 0;
        border-bottom: 1px solid var(--app-border);
        font-size: var(--app-font-sm);
      }
      .metadata-row:last-child {
        border-bottom: 0;
      }
      .metadata dt {
        margin: 0;
        color: var(--app-text-3);
        font-weight: 600;
      }
      .metadata dd {
        margin: 0;
        color: var(--app-text);
        word-break: break-word;
      }
      .conversations {
        list-style: none;
        margin: 0;
        padding: 0;
        display: grid;
        gap: var(--app-space-2);
      }
      .conversation-row {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
      }
      .conversation-row time {
        margin-left: auto;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .has-more-note {
        margin: var(--app-space-2) 0 0;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
        text-align: center;
      }
      @media (max-width: 960px) {
        .profile-grid {
          grid-template-columns: 1fr;
        }
        .profile-grid app-dashboard-card:nth-child(3),
        .profile-grid app-dashboard-card:nth-child(4) {
          grid-column: auto;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CustomerProfileComponent {
  protected readonly store = inject(CustomerProfileStore);
  private readonly route = inject(ActivatedRoute);
  private readonly locale = inject(LocaleService);
  private readonly api = inject(CustomersApiService);
  private readonly permissions = inject(PermissionsService);

  protected readonly customer = this.store.customer;
  protected readonly conversations = this.store.conversations;
  protected readonly loading = this.store.loading;
  protected readonly error = this.store.error;
  protected readonly notFound = this.store.notFound;
  protected readonly hasMoreConversations = this.store.hasMoreConversations;

  protected readonly backLink = `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.customers}`;

  protected readonly headerTitle = computed(
    () => this.customer()?.displayName ?? this.locale.translate(marker('customers.profile.title')),
  );

  protected readonly showLoading = computed(
    () => this.loading() && this.customer() === null && !this.notFound() && this.error() === null,
  );

  protected readonly hasError = computed(() => this.error() !== null || this.notFound());

  protected readonly hasData = computed(() => this.customer() !== null);

  protected readonly identifiers = computed(() => this.customer()?.identifiers ?? []);

  protected readonly metadataEntries = computed<readonly (readonly [string, string])[]>(() => {
    const metadata = this.customer()?.metadata;
    if (!metadata) return [];
    return Object.entries(metadata);
  });

  /** Catalog key of what to tell the person about a failed load. */
  protected readonly errorKey = computed(() => {
    if (this.notFound()) return marker('customers.profile.notFound');
    const error = this.error();
    return error ? errorMessageKey(error) : marker('customers.profile.tryAgain');
  });

  protected readonly canManage = computed(() => this.permissions.has('customers.manage'));

  protected readonly showEditDialog = signal(false);
  protected readonly dialogSubmitting = signal(false);
  protected readonly dialogError = signal<ApiError | null>(null);

  private readonly destroyRef = inject(DestroyRef);

  constructor() {
    onTenantChange(({ to }) => {
      this.closeEditDialog();
      const id = this.route.snapshot.paramMap.get('id');
      if (id && to) {
        this.store.loadProfile(id);
      }
    });

    effect(() => {
      if (this.showEditDialog() && !this.canManage()) {
        this.closeEditDialog();
      }
    });

    this.route.paramMap
      .pipe(
        map((params) => params.get('id')),
        filter((id): id is string => !!id),
        distinctUntilChanged(),
        takeUntilDestroyed(this.destroyRef),
      )
      .subscribe((id) => {
        this.store.loadProfile(id);
      });
  }

  protected openEditDialog(): void {
    this.showEditDialog.set(true);
    this.dialogError.set(null);
  }

  protected closeEditDialog(): void {
    this.showEditDialog.set(false);
    this.dialogSubmitting.set(false);
    this.dialogError.set(null);
  }

  protected onUpdate(payload: UpdateCustomerPayload): void {
    const id = this.store.customerId();
    if (!id) return;
    this.dialogSubmitting.set(true);
    this.dialogError.set(null);
    this.api
      .updateCustomer(id, payload)
      .pipe(
        takeUntilDestroyed(this.destroyRef),
        tap(() => {
          this.closeEditDialog();
          this.store.loadProfile(id);
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

  protected conversationTone(status: ConversationSummary['status']): BadgeTone {
    switch (status) {
      case 'closed':
        return 'green';
      case 'resolved':
        return 'green';
      case 'pending':
        return 'amber';
      default:
        return 'amber';
    }
  }

  protected retry(): void {
    this.store.retry();
  }
}
