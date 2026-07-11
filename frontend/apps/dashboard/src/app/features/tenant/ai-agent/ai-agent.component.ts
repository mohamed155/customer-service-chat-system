import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PAGE_ROUTE, RoutedPageStore } from '../routed-page.store';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { AgentPreviewCardComponent } from '../../../shared/components/ai/agent-preview-card/agent-preview-card.component';
import { AiToolTimelineComponent } from '../../../shared/components/ai/ai-tool-timeline/ai-tool-timeline.component';
import { PromptEditorShellComponent } from '../../../shared/components/ai/prompt-editor-shell/prompt-editor-shell.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { AiAgentStore, AiAgentTab } from './ai-agent.store';

@Component({
  selector: 'app-ai-agent',
  imports: [
    AgentPreviewCardComponent,
    AiToolTimelineComponent,
    DashboardCardComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    PromptEditorShellComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
  ],
  providers: [AiAgentStore, RoutedPageStore, { provide: PAGE_ROUTE, useValue: 'ai-agent' }],
  template: `
    <app-page-container>
      <app-page-header title="AI Agent" [description]="'Configure how your assistant behaves'" />
      @if (page.loading()) {
        <app-loading-state />
      } @else if (hasError()) {
        <app-empty-state
          icon="@tui.alert-circle"
          title="Something went wrong"
          description="We couldn't load this page. Please try again."
        >
          <button type="button" (click)="retry()">Try again</button>
        </app-empty-state>
      } @else if (hasData()) {
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

        @switch (store.activeTab()) {
          @case ('behavior') {
            <section class="grid two">
              <app-dashboard-card>
                <app-section-header
                  card-header
                  title="Agent profile"
                  subtitle="Default behavior and response style"
                />
                <label>Name<input value="Helix Assistant" /></label>
                <label
                  >Tone<select>
                    <option>Calm and concise</option>
                    <option>Warm</option>
                  </select></label
                >
                <label
                  >Language<select>
                    <option>Match customer language</option>
                    <option>English</option>
                  </select></label
                >
                <label
                  >Response length<select>
                    <option>Brief</option>
                    <option>Detailed</option>
                  </select></label
                >
              </app-dashboard-card>
              <app-dashboard-card>
                <app-section-header
                  card-header
                  title="Behavior guardrails"
                  subtitle="Visual configuration for safe answers"
                />
                <div class="chips">
                  @for (topic of pageData()?.allowedTopics; track topic) {
                    <app-status-badge [status]="topic" tone="green" />
                  }
                </div>
                <div class="chips">
                  @for (topic of pageData()?.blockedTopics; track topic) {
                    <app-status-badge [status]="topic" tone="red" />
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
                subtitle="Mono editor shell for future prompt versioning"
              />
              <app-prompt-editor-shell [(value)]="prompt" />
            </app-dashboard-card>
          }
          @case ('escalation') {
            <app-dashboard-card>
              <app-section-header
                card-header
                title="Escalation triggers"
                subtitle="Signals that move conversations to humans"
              />
              <div class="rules">
                @for (rule of pageData()?.escalationRules; track rule) {
                  <article>{{ rule }}</article>
                }
              </div>
            </app-dashboard-card>
          }
          @case ('testing') {
            <section class="grid two">
              <app-dashboard-card>
                <app-section-header
                  card-header
                  title="Test assistant"
                  subtitle="Static transcript preview"
                />
                <app-agent-preview-card />
              </app-dashboard-card>
              <app-dashboard-card>
                <app-section-header
                  card-header
                  title="Tool timeline"
                  subtitle="Inspectable execution shape"
                />
                <app-ai-tool-timeline [steps]="pageData()?.timelineSteps ?? []" />
              </app-dashboard-card>
            </section>
          }
        }
      } @else {
        <app-empty-state
          icon="@tui.bot"
          title="No agent configuration"
          description="Configure your AI agent's behavior, knowledge sources, and escalation rules to get started."
        />
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
      }
      .two {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }
      label {
        display: grid;
        gap: 6px;
        margin-bottom: var(--app-space-3);
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        font-weight: 650;
      }
      input,
      select {
        height: 38px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
        font: inherit;
      }
      .chips,
      .rules {
        display: flex;
        gap: var(--app-space-2);
        flex-wrap: wrap;
        margin-bottom: var(--app-space-4);
      }
      .rules {
        display: grid;
      }
      .rules article {
        padding: var(--app-space-3);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
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
  protected readonly page = inject(RoutedPageStore);
  protected readonly hasData = computed(() => this.page.data() !== undefined);
  protected readonly hasError = computed(() => this.page.error() !== null);
  protected readonly store = inject(AiAgentStore);
  protected readonly prompt = signal(
    'You are Helix, a concise AI support assistant.\nUse trusted knowledge citations.\nEscalate when confidence is low.',
  );
  protected readonly tabs: readonly { id: AiAgentTab; label: string }[] = [
    { id: 'behavior', label: 'Behavior' },
    { id: 'prompt', label: 'Prompt' },
    { id: 'escalation', label: 'Escalation' },
    { id: 'testing', label: 'Testing' },
  ];

  protected readonly pageData = computed(() => {
    const data = this.page.data();
    if (data?.page === 'ai-agent') return data.data;
    return undefined;
  });

  protected retry(): void {
    this.page.retry();
  }
}
