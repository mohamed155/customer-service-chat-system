import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
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
    PageContainerComponent,
    PromptEditorShellComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
  ],
  providers: [AiAgentStore],
  template: `
    <app-page-container>
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
                @for (topic of allowedTopics; track topic) {
                  <app-status-badge [status]="topic" tone="green" />
                }
              </div>
              <div class="chips">
                @for (topic of blockedTopics; track topic) {
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
              @for (rule of escalationRules; track rule) {
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
              <app-ai-tool-timeline [steps]="timelineSteps" />
            </app-dashboard-card>
          </section>
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
  protected readonly allowedTopics = ['Shipping', 'Returns', 'Billing', 'Warranty'];
  protected readonly blockedTopics = ['Legal advice', 'Medical claims', 'Payment secrets'];
  protected readonly escalationRules = [
    'Customer sentiment is angry and confidence drops below 75%',
    'Repeated answer loop detected within two AI turns',
    'Warranty or billing policy conflict requires human review',
  ];
  protected readonly timelineSteps = [
    { label: 'Classify intent', detail: 'Exchange request with promotional credit' },
    { label: 'Retrieve knowledge', detail: 'Returns and exchanges policy' },
    { label: 'Draft answer', detail: 'Preserve eligible credit and explain next step' },
  ];
}
