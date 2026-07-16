import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { AvatarPickerComponent, AvatarValue } from './avatar-picker.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { PromptEditorComponent } from './prompt-editor.component';
import {
  ProviderModelSelectorComponent,
  ProviderModelValue,
} from './provider-model-selector.component';
import { RulesEditorComponent, EscalationRuleEdit } from './rules-editor.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { ToneSelectorComponent } from './tone-selector.component';
import { AiAgentStore, AiAgentTab } from './ai-agent.store';

@Component({
  selector: 'app-ai-agent',
  imports: [
    AvatarPickerComponent,
    DashboardCardComponent,
    EmptyStateComponent,
    FormsModule,
    InlineAlertComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    PromptEditorComponent,
    ProviderModelSelectorComponent,
    RulesEditorComponent,
    SectionHeaderComponent,
    ToneSelectorComponent,
  ],
  providers: [AiAgentStore],
  template: `
    <app-page-container>
      <app-page-header title="AI Agent" [description]="'Configure how your assistant behaves'" />

      @if (store.loading() && !store.config()) {
        <app-loading-state />
      } @else if (store.error() && !store.config()) {
        <app-empty-state
          icon="@tui.alert-circle"
          title="Something went wrong"
          description="We couldn't load this page. Please try again."
        >
          <button type="button" (click)="store.load()">Try again</button>
        </app-empty-state>
      } @else {
        <div class="tabs" role="tablist" aria-label="AI Agent sections">
          @for (tab of tabs; track tab.id) {
            <button
              type="button"
              role="tab"
              [attr.aria-selected]="store.activeTab() === tab.id"
              [class.active]="store.activeTab() === tab.id"
              (click)="store.setTab(tab.id)"
            >
              {{ tab.label }}
            </button>
          }
        </div>

        @if (store.hasConflict()) {
          <div class="conflict-banner">
            <app-inline-alert tone="error">
              Updated since loaded. Please
              <button
                type="button"
                class="link-btn"
                (click)="store.load(); store.dismissConflict()"
              >
                reload
              </button>
              before saving.
            </app-inline-alert>
          </div>
        }

        @if (!store.isConfigured()) {
          <app-inline-alert tone="info">Not yet configured — showing defaults</app-inline-alert>
        }

        @if ((store.config()?.agent?.enabledChannels?.length ?? 0) === 0) {
          <app-inline-alert tone="info">Agent inactive — no channels enabled</app-inline-alert>
        }

        @switch (store.activeTab()) {
          @case ('behavior') {
            <section class="grid two">
              <app-dashboard-card>
                <app-section-header
                  card-header
                  title="Agent profile"
                  subtitle="Identity, tone, and avatar"
                />
                <label class="field-label">
                  Name
                  <input
                    [ngModel]="agentName()"
                    (ngModelChange)="agentName.set($event)"
                    placeholder="Agent name"
                  />
                </label>
                @if (fieldError('name'); as errors) {
                  @for (err of errors; track err) {
                    <app-inline-alert tone="error">{{ err }}</app-inline-alert>
                  }
                }
                <app-avatar-picker
                  [presets]="store.options()?.avatarPresets ?? []"
                  [(value)]="agentAvatar"
                />
                <app-tone-selector [tones]="store.options()?.tones ?? []" [(value)]="agentTone" />
              </app-dashboard-card>
              <app-dashboard-card>
                <app-section-header
                  card-header
                  title="Provider & Model"
                  subtitle="Select the AI provider and model"
                />
                <app-provider-model-selector
                  [providers]="agentProviders()"
                  [(value)]="agentProviderModel"
                  [stale]="store.staleProviderSelection()"
                />
                @if (fieldError('providerSelection'); as errors) {
                  @for (err of errors; track err) {
                    <app-inline-alert tone="error">{{ err }}</app-inline-alert>
                  }
                }
              </app-dashboard-card>
            </section>

            <section class="grid">
              <app-dashboard-card>
                <app-section-header
                  card-header
                  title="Channels"
                  subtitle="Enable channels for this agent"
                />
                <div class="channel-list">
                  @for (ch of store.options()?.channels ?? []; track ch) {
                    <label class="channel-toggle">
                      <input
                        type="checkbox"
                        [checked]="agentEnabledChannels().includes(ch)"
                        (change)="toggleChannel(ch)"
                      />
                      {{ ch }}
                    </label>
                  }
                </div>
              </app-dashboard-card>
            </section>
          }
          @case ('prompt') {
            <app-dashboard-card>
              <app-section-header
                card-header
                title="System prompt"
                subtitle="The base instruction given to the AI for every conversation"
              />
              <app-prompt-editor
                [(value)]="agentPrompt"
                [maxLength]="store.options()?.promptMaxLength ?? 8000"
              />
              @if (fieldError('systemPrompt'); as errors) {
                @for (err of errors; track err) {
                  <app-inline-alert tone="error">{{ err }}</app-inline-alert>
                }
              }
            </app-dashboard-card>
          }
          @case ('escalation') {
            <app-dashboard-card>
              <app-section-header
                card-header
                title="Escalation rules"
                subtitle="Define when and how conversations escalate to humans"
              />
              <app-rules-editor
                [(businessRules)]="agentBusinessRules"
                [(escalationRules)]="agentEscalationRules"
                [brokenSkillRefs]="store.brokenSkillRefs()"
              />
              @if (fieldError('businessRules'); as errors) {
                @for (err of errors; track err) {
                  <app-inline-alert tone="error">{{ err }}</app-inline-alert>
                }
              }
              @if (fieldError('escalationRules'); as errors) {
                @for (err of errors; track err) {
                  <app-inline-alert tone="error">{{ err }}</app-inline-alert>
                }
              }
            </app-dashboard-card>
          }
        }

        <div class="actions-bar">
          @if (store.saving()) {
            <span class="saving-indicator">Saving…</span>
          }
          <button type="button" class="save-btn" [disabled]="store.saving()" (click)="save()">
            Save
          </button>
        </div>

        @if (store.error() && store.config()) {
          <app-inline-alert tone="error">{{ store.error() }}</app-inline-alert>
        }
      }
    </app-page-container>
  `,
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

  protected readonly tabs: readonly { id: AiAgentTab; label: string }[] = [
    { id: 'behavior', label: 'Behavior' },
    { id: 'prompt', label: 'Prompt' },
    { id: 'escalation', label: 'Escalation' },
  ];

  protected readonly agentName = signal('');
  protected readonly agentAvatar = signal<AvatarValue>(null);
  protected readonly agentTone = signal('');
  protected readonly agentPrompt = signal('');
  protected readonly agentBusinessRules = signal<string[]>([]);
  protected readonly agentEscalationRules = signal<EscalationRuleEdit[]>([]);
  protected readonly agentEnabledChannels = signal<string[]>([]);
  protected readonly agentProviderModel = signal<ProviderModelValue>({
    providerId: null,
    model: null,
  });

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
        this.agentPrompt.set(config.agent.systemPrompt);
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
      systemPrompt: this.agentPrompt(),
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

  protected fieldError(field: string): string[] | null {
    return this.store.fieldErrors()?.[field] ?? null;
  }
}
