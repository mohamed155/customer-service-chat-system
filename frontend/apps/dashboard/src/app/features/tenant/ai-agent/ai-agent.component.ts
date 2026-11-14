import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { errorMessageKey } from '../../../core/errors/error-message';
import { LocaleDatePipe } from '../../../core/i18n/format';
import { channelLabelKey } from '../../../core/i18n/labels';
import { marker } from '../../../core/i18n/marker';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { AvatarPickerComponent, AvatarValue } from './avatar-picker.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../../core/router/app-paths';
import {
  ProviderModelSelectorComponent,
  ProviderModelValue,
} from './provider-model-selector.component';
import { RulesEditorComponent, EscalationRuleEdit } from './rules-editor.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { ToneSelectorComponent } from './tone-selector.component';
import { AiAgentStore, AiAgentTab } from './ai-agent.store';
import { ApiError } from '../../../core/api/api.models';

@Component({
  selector: 'app-ai-agent',
  imports: [
    AvatarPickerComponent,
    DashboardCardComponent,
    EmptyStateComponent,
    ErrorReferenceComponent,
    FormsModule,
    InlineAlertComponent,
    LoadingStateComponent,
    LocaleDatePipe,
    PageContainerComponent,
    PageHeaderComponent,
    ProviderModelSelectorComponent,
    RouterLink,
    RulesEditorComponent,
    SectionHeaderComponent,
    ToneSelectorComponent,
    TranslocoPipe,
  ],
  providers: [AiAgentStore, provideTranslocoScope('ai-agent')],
  templateUrl: './ai-agent.component.html',
  styles: [
    `
      .tabs {
        display: flex;
        gap: var(--app-space-1);
        margin-bottom: var(--app-space-5);
        padding: 4px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
        width: fit-content;
      }
      .tabs button {
        height: 34px;
        padding: 0 var(--app-space-3);
        border: 0;
        border-radius: var(--app-radius-md);
        background: transparent;
        color: var(--app-text-2);
        font-weight: 650;
        cursor: pointer;
      }
      .tabs button.active {
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      .grid {
        display: grid;
        gap: var(--app-space-4);
        margin-bottom: var(--app-space-4);
      }
      .two {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }
      .field-label {
        display: grid;
        gap: 6px;
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        font-weight: 650;
      }
      .field-label input {
        height: 38px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        font: inherit;
      }
      .channel-list {
        display: flex;
        gap: var(--app-space-4);
        flex-wrap: wrap;
      }
      .channel-toggle {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        font-size: var(--app-font-sm);
        color: var(--app-text);
        cursor: pointer;
      }
      .channel-toggle input {
        width: 18px;
        height: 18px;
        accent-color: var(--app-accent);
      }
      .actions-bar {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: var(--app-space-3);
        margin-top: var(--app-space-5);
        padding-top: var(--app-space-4);
        border-top: 1px solid var(--app-border);
      }
      .save-btn {
        height: 38px;
        padding: 0 var(--app-space-5);
        border: 0;
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 650;
        font-size: var(--app-font-sm);
        cursor: pointer;
      }
      .save-btn:disabled {
        opacity: 0.6;
        cursor: default;
      }
      .save-btn:hover:not(:disabled) {
        opacity: 0.92;
      }
      .saving-indicator {
        font-size: var(--app-font-sm);
        color: var(--app-text-2);
      }
      .conflict-banner {
        margin-bottom: var(--app-space-3);
      }
      .link-btn {
        background: none;
        border: none;
        color: inherit;
        text-decoration: underline;
        cursor: pointer;
        font: inherit;
        padding: 0;
      }
      .prompt-summary {
        display: grid;
        gap: var(--app-space-2);
        margin-bottom: var(--app-space-3);
      }
      .prompt-meta {
        display: flex;
        gap: var(--app-space-2);
        font-size: var(--app-font-sm);
      }
      .meta-label {
        color: var(--app-text-2);
        font-weight: 650;
        min-width: 72px;
      }
      .meta-value {
        color: var(--app-text);
      }
      .prompt-excerpt {
        padding: var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        font-family: monospace;
        line-height: 1.5;
        white-space: pre-wrap;
        word-break: break-word;
        max-height: 120px;
        overflow-y: auto;
      }
      .prompt-unset {
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        font-style: italic;
        margin-bottom: var(--app-space-3);
      }
      .prompt-actions {
        display: flex;
        gap: var(--app-space-3);
      }
      .manage-link {
        display: inline-flex;
        align-items: center;
        height: 38px;
        padding: 0 var(--app-space-5);
        border: 0;
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 650;
        font-size: var(--app-font-sm);
        text-decoration: none;
        cursor: pointer;
      }
      .manage-link:hover {
        opacity: 0.92;
      }
      @media (max-width: 900px) {
        .two {
          grid-template-columns: 1fr;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AiAgentComponent {
  readonly store = inject(AiAgentStore);
  protected readonly APP_PATHS = APP_PATHS;

  protected readonly tabs: readonly { id: AiAgentTab; key: string }[] = [
    { id: 'behavior', key: marker('ai-agent.tabs.behavior') },
    { id: 'prompt', key: marker('ai-agent.tabs.prompt') },
    { id: 'escalation', key: marker('ai-agent.tabs.escalation') },
  ];

  protected readonly agentName = signal('');
  protected readonly agentAvatar = signal<AvatarValue>(null);
  protected readonly agentTone = signal('');
  protected readonly agentBusinessRules = signal<string[]>([]);
  protected readonly agentEscalationRules = signal<EscalationRuleEdit[]>([]);
  protected readonly agentEnabledChannels = signal<string[]>([]);
  protected readonly agentProviderModel = signal<ProviderModelValue>({
    providerId: null,
    model: null,
  });

  /** The channels the agent can use, each with the catalog key of its name (none: shown as sent). */
  protected readonly channels = computed(() =>
    (this.store.options()?.channels ?? []).map((id) => ({ id, key: channelLabelKey(id) })),
  );

  protected readonly agentProviders = computed(() =>
    (this.store.options()?.providers ?? []).map((p) => ({
      id: p.provider,
      name: p.provider,
      credentialAvailable: p.credentialAvailable,
      models: p.models,
    })),
  );

  constructor() {
    effect(() => {
      const config = this.store.config();
      if (config) {
        this.agentName.set(config.agent.name);
        this.agentTone.set(config.agent.tone);
        this.agentBusinessRules.set(config.agent.businessRules);
        this.agentEscalationRules.set(config.agent.escalationRules);
        this.agentEnabledChannels.set(config.agent.enabledChannels);
        this.agentAvatar.set(
          config.agent.avatar.kind === 'preset' && config.agent.avatar.preset
            ? { kind: 'preset', preset: config.agent.avatar.preset }
            : { kind: 'upload' },
        );
        this.agentProviderModel.set({
          providerId: config.agent.providerSelection.provider,
          model: config.agent.providerSelection.model,
        });
      }
    });
  }

  protected save(): void {
    const avatar = this.agentAvatar();
    this.store.save({
      name: this.agentName(),
      avatar:
        avatar?.kind === 'preset' ? { kind: 'preset', preset: avatar.preset } : { kind: 'upload' },
      tone: this.agentTone(),
      businessRules: this.agentBusinessRules(),
      escalationRules: this.agentEscalationRules(),
      enabledChannels: this.agentEnabledChannels(),
      providerSelection: this.agentProviderModel().providerId
        ? {
            provider: this.agentProviderModel().providerId!,
            model: this.agentProviderModel().model ?? '',
          }
        : null,
      version: this.store.config()?.agent.version ?? null,
    });
  }

  protected toggleChannel(channel: string): void {
    this.agentEnabledChannels.update((list) =>
      list.includes(channel) ? list.filter((c) => c !== channel) : [...list, channel],
    );
  }

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  protected fieldError(field: string): string[] | null {
    return this.store.fieldErrors()?.[field] ?? null;
  }
}
