import { DatePipe } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
  untracked,
} from '@angular/core';
import { RouterLink } from '@angular/router';
import { FormsModule } from '@angular/forms';
import { Permission } from '../../../core/authz/permissions';
import { PermissionsService } from '../../../core/authz/permissions.service';
import {
  IntegrationConfigField,
  IntegrationDetail,
  IntegrationListItem,
} from '../../../core/api/tenant-api.models';
import { APP_PATHS } from '../../../core/router/app-paths';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import {
  BadgeTone,
  StatusBadgeComponent,
} from '../../../shared/components/status-badge/status-badge.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { copyToClipboard } from '../../../shared/utils/clipboard';
import { relativeTime } from '../../../shared/utils/relative-time';
import { IntegrationDetailStore } from './integration-detail.store';

interface FormValues {
  readonly config: Record<string, string>;
  readonly secrets: Record<string, string>;
}

function emptyValues(): FormValues {
  return { config: {}, secrets: {} };
}

function textValueFor(field: IntegrationConfigField, detail: IntegrationDetail | null): string {
  if (!detail?.connection) return '';
  const v = detail.connection.config[field.key];
  return typeof v === 'string' ? v : v == null ? '' : String(v);
}

function secretHintFor(
  field: IntegrationConfigField,
  detail: IntegrationDetail | null,
): string | null {
  if (!detail?.connection) return null;
  const ref = detail.connection.secrets.find((s) => s.fieldKey === field.key);
  return ref?.hint ?? null;
}

@Component({
  selector: 'app-integration-detail',
  imports: [
    ButtonComponent,
    DatePipe,
    DashboardCardComponent,
    EmptyStateComponent,
    FormFieldComponent,
    FormsModule,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    RouterLink,
    SectionHeaderComponent,
    StatusBadgeComponent,
  ],
  templateUrl: './integration-detail.component.html',
  styles: [
    `
      .back {
        display: inline-block;
        color: var(--app-text-2);
        text-decoration: none;
        font-size: var(--app-font-sm);
        margin-bottom: var(--app-space-3);
      }
      .summary {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        margin: var(--app-space-4) 0;
      }
      .category {
        padding: 4px var(--app-space-2);
        border-radius: var(--app-radius-sm);
        background: var(--app-panel-2);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        font-weight: 600;
        text-transform: uppercase;
      }
      .badge-soon {
        display: inline-block;
        padding: 2px var(--app-space-2);
        border-radius: var(--app-radius-sm);
        background: var(--app-panel-2);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        font-weight: 600;
      }
      .config-form {
        display: grid;
        gap: var(--app-space-4);
      }
      .actions {
        display: flex;
        gap: var(--app-space-2);
        flex-wrap: wrap;
      }
      .form-error {
        color: var(--app-red);
        font-size: var(--app-font-sm);
        margin: 0;
      }
      .secret-hint {
        font-size: var(--app-font-xs);
        color: var(--app-text-2);
      }
      dl.connection {
        display: grid;
        gap: var(--app-space-3);
        margin: 0;
      }
      .row {
        display: grid;
        grid-template-columns: 220px 1fr;
        gap: var(--app-space-4);
        align-items: baseline;
      }
      dt {
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
      }
      dd {
        margin: 0;
        color: var(--app-text);
        font-size: var(--app-font-sm);
      }
      .webhook {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        flex-wrap: wrap;
      }
      .secrets {
        list-style: none;
        padding: 0;
        margin: 0;
      }
      code {
        font-family: var(--app-font-mono, monospace);
        font-size: var(--app-font-xs);
        word-break: break-all;
      }
      .muted {
        color: var(--app-text-2);
        margin: 0;
      }
      .events {
        list-style: none;
        padding: 0;
        margin: 0;
        display: grid;
        gap: var(--app-space-2);
      }
      .event {
        padding: var(--app-space-3);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
      }
      .event-row {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        flex-wrap: wrap;
      }
      .event-type {
        font-weight: 600;
        color: var(--app-text);
        font-size: var(--app-font-sm);
      }
      .event-time {
        margin-left: auto;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
        white-space: nowrap;
      }
      .event-reason {
        margin: 4px 0 0;
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
      }
      .load-more {
        display: flex;
        justify-content: center;
        padding-top: var(--app-space-3);
      }
    `,
  ],
  providers: [IntegrationDetailStore],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class IntegrationDetailComponent {
  protected readonly slug = input.required<string>();
  protected readonly store = inject(IntegrationDetailStore);
  private readonly permissions = inject(PermissionsService);

  protected readonly detail = this.store.detail;
  protected readonly loading = this.store.loading;
  protected readonly error = this.store.error;
  protected readonly saving = this.store.saving;
  protected readonly storeError = computed(() => this.error());
  protected readonly events = this.store.events;
  protected readonly eventsLoading = this.store.eventsLoading;
  protected readonly eventsError = this.store.eventsError;
  protected readonly eventsHasMore = this.store.eventsHasMore;

  protected readonly formValues = signal<FormValues>(emptyValues());
  protected readonly copyStatus = signal<'idle' | 'copied' | 'failed'>('idle');
  private copyRequest = 0;

  protected readonly title = computed(() => this.detail()?.name ?? 'Loading…');
  protected readonly description = computed(() => this.detail()?.description ?? '');

  protected readonly backLink = computed(() => [
    '/',
    APP_PATHS.tenant.base,
    APP_PATHS.tenant.integrations,
  ]);

  protected readonly canManage = computed(() =>
    this.permissions.has('integrations.manage' as Permission),
  );

  constructor() {
    effect(() => {
      const s = this.slug();
      untracked(() => {
        this.store.load(s);
        this.store.loadFirstPageEvents(s);
      });
    });

    effect(() => {
      const d = this.detail();
      if (!d) {
        untracked(() => this.formValues.set(emptyValues()));
        return;
      }
      const config: Record<string, string> = {};
      for (const field of d.configSchema) {
        if (field.kind === 'text') {
          config[field.key] = textValueFor(field, d);
        }
      }
      untracked(() => this.formValues.set({ config, secrets: {} }));
    });
  }

  protected tone(status: IntegrationListItem['status']): BadgeTone {
    switch (status) {
      case 'connected':
        return 'green';
      case 'error':
        return 'red';
      case 'disconnected':
        return 'amber';
      default:
        return 'neutral';
    }
  }

  protected outcomeTone(outcome: string): BadgeTone {
    return outcome === 'success' ? 'green' : outcome === 'failure' ? 'red' : 'neutral';
  }

  protected formatEventTime(dateStr: string): string {
    return relativeTime(dateStr);
  }

  protected onLoadMoreEvents(): void {
    this.store.loadMoreEvents(this.slug());
  }

  protected retryEvents(): void {
    this.store.loadFirstPageEvents(this.slug());
  }

  protected retry(): void {
    this.store.load(this.slug());
  }

  protected onConfigChange(key: string, value: string): void {
    const current = this.formValues();
    this.formValues.set({
      config: { ...current.config, [key]: value },
      secrets: current.secrets,
    });
  }

  protected onSecretChange(key: string, value: string): void {
    const current = this.formValues();
    const nextSecrets = { ...current.secrets };
    if (value === '') {
      delete nextSecrets[key];
    } else {
      nextSecrets[key] = value;
    }
    this.formValues.set({
      config: current.config,
      secrets: nextSecrets,
    });
  }

  protected secretHint(key: string): string | null {
    return secretHintFor(
      { key, label: '', kind: 'secret', required: false } as IntegrationConfigField,
      this.detail(),
    );
  }

  protected onSubmit(): void {
    const slug = this.slug();
    const d = this.detail();
    if (!d) return;
    const { config, secrets } = this.formValues();
    const isConnected =
      !!d.connection && d.status !== 'disconnected' && d.status !== 'not_connected';
    if (isConnected) {
      const hasSecretChange = Object.keys(secrets).length > 0;
      this.store.updateConfig(slug, {
        config,
        ...(hasSecretChange ? { secrets } : {}),
      });
    } else {
      this.store.connect(slug, { config, secrets });
    }
  }

  protected onDisconnect(): void {
    this.store.disconnect(this.slug());
  }

  protected async onCopyWebhook(url: string): Promise<void> {
    if (!url) return;
    const request = ++this.copyRequest;
    this.copyStatus.set('idle');
    try {
      await copyToClipboard(url);
      if (request === this.copyRequest) this.copyStatus.set('copied');
      setTimeout(() => {
        if (request === this.copyRequest) this.copyStatus.set('idle');
      }, 2000);
    } catch {
      if (request === this.copyRequest) this.copyStatus.set('failed');
    }
  }
}
