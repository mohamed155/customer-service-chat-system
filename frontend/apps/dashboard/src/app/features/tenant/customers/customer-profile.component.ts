import { DatePipe } from '@angular/common';
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
import { TenantContextService } from '../../../core/tenant/tenant-context.service';
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
import { CustomersApiService } from './customers-api.service';
import { CustomerDialogComponent } from './customer-dialog.component';
import { CustomerProfileStore } from './customer-profile.store';

@Component({
  selector: 'app-customer-profile',
  imports: [
    DatePipe,
    RouterLink,
    AvatarComponent,
    ButtonComponent,
    ChannelBadgeComponent,
    CustomerDialogComponent,
    DashboardCardComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
  ],
  template: `
    <app-page-container>
      <a [routerLink]="backLink" class="back-link">← Back to customers</a>

      <app-page-header
        [title]="headerTitle()"
        description="Contact, identifiers, metadata, and conversation history"
      >
        @if (canManage() && hasData()) {
          <app-button toolbar-end variant="primary" (pressed)="openEditDialog()">Edit</app-button>
        }
      </app-page-header>

      @if (showLoading()) {
        <app-loading-state label="Loading customer..." />
      } @else if (hasError()) {
        <app-empty-state
          icon="@tui.alert-circle"
          title="Something went wrong"
          [description]="errorMessage()"
        >
          <app-button (pressed)="retry()">Try again</app-button>
        </app-empty-state>
      } @else if (hasData()) {
        <div class="profile-grid">
          <app-dashboard-card>
            <app-section-header card-header title="Contact" />
            <div class="contact">
              <app-avatar [initials]="initials(customer()!.displayName)" size="lg" />
              <div class="contact-body">
                <strong class="name">{{ customer()!.displayName }}</strong>
                @if (customer()!.email) {
                  <div class="contact-row">
                    <span class="label">Email</span>
                    <span>{{ customer()!.email }}</span>
                  </div>
                }
                @if (customer()!.phone) {
                  <div class="contact-row">
                    <span class="label">Phone</span>
                    <span>{{ customer()!.phone }}</span>
                  </div>
                }
              </div>
            </div>
            <div class="timestamps">
              <div>
                <span class="label">Created</span>
                <time>{{ customer()!.createdAt | date: 'medium' }}</time>
              </div>
              <div>
                <span class="label">Updated</span>
                <time>{{ customer()!.updatedAt | date: 'medium' }}</time>
              </div>
            </div>
          </app-dashboard-card>

          <app-dashboard-card>
            <app-section-header card-header title="Channel identifiers" />
            @if (identifiers().length === 0) {
              <app-empty-state
                icon="@tui.waypoints"
                title="No channel identifiers"
                description="This customer has no channel identifiers configured."
              />
            } @else {
              <ul class="identifiers">
                @for (identifier of identifiers(); track identifier.id) {
                  <li>
                    <app-channel-badge [channel]="identifier.channel" />
                    <span class="identifier-value">{{ identifier.identifier }}</span>
                  </li>
                }
              </ul>
            }
          </app-dashboard-card>

          <app-dashboard-card>
            <app-section-header card-header title="Metadata" />
            @if (metadataEntries().length === 0) {
              <app-empty-state
                icon="@tui.list"
                title="No metadata"
                description="This customer has no metadata entries."
              />
            } @else {
              <dl class="metadata">
                @for (entry of metadataEntries(); track entry[0]) {
                  <div class="metadata-row">
                    <dt>{{ entry[0] }}</dt>
                    <dd>{{ entry[1] }}</dd>
                  </div>
                }
              </dl>
            }
          </app-dashboard-card>

          <app-dashboard-card>
            <app-section-header
              card-header
              title="Conversation history"
              subtitle="Recent conversations, newest first"
            />
            @if (conversations().length === 0) {
              <app-empty-state
                icon="@tui.message-circle"
                title="No conversations yet"
                description="This customer hasn't had any conversations."
              />
            } @else {
              <ul class="conversations">
                @for (conversation of conversations(); track conversation.id) {
                  <li class="conversation-row">
                    <app-channel-badge [channel]="conversation.channel" />
                    <app-status-badge
                      [status]="conversation.status"
                      [tone]="conversationTone(conversation.status)"
                    />
                    <time>{{ conversation.lastActivityAt | date: 'short' }}</time>
                  </li>
                }
              </ul>
              @if (hasMoreConversations()) {
                <p class="has-more-note">
                  Showing {{ conversations().length }} most recent conversations.
                </p>
              }
            }
          </app-dashboard-card>
        </div>
      }
    </app-page-container>

    @if (showEditDialog()) {
      <app-customer-dialog
        mode="edit"
        [customer]="customer()"
        [submitting]="dialogSubmitting()"
        [error]="dialogError()"
        (update)="onUpdate($event)"
        (closeDialog)="closeEditDialog()"
      />
    }
  `,
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
    () => this.customer()?.displayName ?? 'Customer profile',
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

  protected readonly errorMessage = computed(
    () =>
      this.error()?.message ??
      (this.notFound() ? 'This customer could not be found.' : 'Please try again.'),
  );

  protected readonly canManage = computed(() => this.permissions.has('customers.manage'));

  protected readonly showEditDialog = signal(false);
  protected readonly dialogSubmitting = signal(false);
  protected readonly dialogError = signal<ApiError | null>(null);

  private readonly destroyRef = inject(DestroyRef);
  private readonly tenantContext = inject(TenantContextService);

  constructor() {
    let tenantInitialized = false;

    effect(() => {
      this.tenantContext.activeTenant();
      if (!tenantInitialized) {
        tenantInitialized = true;
        return;
      }
      this.closeEditDialog();
      const id = this.route.snapshot.paramMap.get('id');
      if (id) {
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
