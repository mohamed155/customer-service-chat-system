import { ChangeDetectionStrategy, Component, input, model } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';

export interface EscalationRuleEdit {
  id?: string;
  name: string;
  trigger: 'human_request' | 'topic_keywords';
  keywords: string[];
  requiredSkillIds: string[];
  brokenSkillRefs?: string[];
}

@Component({
  selector: 'app-rules-editor',
  standalone: true,
  imports: [FormsModule, ButtonComponent, InlineAlertComponent],
  template: `
    <section class="section">
      <h3 class="heading">Business Rules</h3>
      <p class="subtitle">Guidelines the AI follows when responding</p>

      <div class="rule-list">
        @for (rule of businessRules(); track $index; let i = $index) {
          <div class="rule-row">
            <input [ngModel]="rule" (ngModelChange)="updateBusinessRule(i, $event)" />
            <app-button variant="danger" size="sm" (pressed)="removeBusinessRule(i)">
              Remove
            </app-button>
          </div>
        }
      </div>

      <app-button variant="ghost" size="sm" (pressed)="addBusinessRule()"> + Add rule </app-button>
    </section>

    <section class="section">
      <h3 class="heading">Escalation Rules</h3>
      <p class="subtitle">When and how to transfer to a human agent</p>

      <div class="rule-list">
        @for (rule of escalationRules(); track $index; let i = $index) {
          <fieldset class="escalation-rule">
            <legend>
              Escalation {{ i + 1 }}
              <app-button variant="danger" size="sm" (pressed)="removeEscalationRule(i)">
                Remove
              </app-button>
            </legend>

            @for (ref of rule.brokenSkillRefs ?? []; track ref) {
              <app-inline-alert tone="error"> Unknown skill reference: {{ ref }} </app-inline-alert>
            }

            <label class="field">
              Name
              <input [ngModel]="rule.name" (ngModelChange)="updateEscalationName(i, $event)" />
            </label>

            <fieldset class="radio-group">
              <legend>Trigger</legend>
              <label class="radio">
                <input
                  type="radio"
                  name="trigger-{{ i }}"
                  value="human_request"
                  [checked]="rule.trigger === 'human_request'"
                  (change)="updateEscalationTrigger(i, 'human_request')"
                />
                Human request
              </label>
              <label class="radio">
                <input
                  type="radio"
                  name="trigger-{{ i }}"
                  value="topic_keywords"
                  [checked]="rule.trigger === 'topic_keywords'"
                  (change)="updateEscalationTrigger(i, 'topic_keywords')"
                />
                Topic keywords
              </label>
            </fieldset>

            <label class="field">
              Keywords (comma separated)
              <input
                [ngModel]="rule.keywords.join(', ')"
                (ngModelChange)="updateEscalationKeywords(i, $event)"
              />
            </label>

            <label class="field">
              Required Skill IDs (comma separated)
              <input
                [ngModel]="rule.requiredSkillIds.join(', ')"
                (ngModelChange)="updateEscalationSkills(i, $event)"
              />
            </label>
          </fieldset>
        }
      </div>

      <app-button variant="ghost" size="sm" (pressed)="addEscalationRule()">
        + Add escalation rule
      </app-button>
    </section>
  `,
  styles: [
    `
      :host {
        display: grid;
        gap: var(--app-space-6);
      }
      .section {
        display: grid;
        gap: var(--app-space-3);
      }
      .heading {
        margin: 0;
        font-size: var(--app-font-md);
        font-weight: 700;
        color: var(--app-text);
      }
      .subtitle {
        margin: 0;
        font-size: var(--app-font-sm);
        color: var(--app-text-2);
      }
      .rule-list {
        display: grid;
        gap: var(--app-space-2);
      }
      .rule-row {
        display: flex;
        gap: var(--app-space-2);
        align-items: center;
      }
      .rule-row input {
        flex: 1;
        height: 38px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font: inherit;
      }
      .escalation-rule {
        display: grid;
        gap: var(--app-space-3);
        padding: var(--app-space-4);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
      }
      .escalation-rule legend {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        font-weight: 700;
        color: var(--app-text);
      }
      .field {
        display: grid;
        gap: var(--app-space-1);
        font-size: var(--app-font-sm);
        font-weight: 600;
        color: var(--app-text-2);
      }
      .field input {
        height: 38px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        font: inherit;
      }
      .radio-group {
        border: 0;
        padding: 0;
        display: flex;
        gap: var(--app-space-4);
      }
      .radio-group legend {
        font-size: var(--app-font-sm);
        font-weight: 600;
        color: var(--app-text-2);
        margin-bottom: var(--app-space-1);
      }
      .radio {
        display: flex;
        align-items: center;
        gap: var(--app-space-1);
        font-size: var(--app-font-sm);
        color: var(--app-text);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RulesEditorComponent {
  readonly businessRules = model<string[]>([]);
  readonly escalationRules = model<EscalationRuleEdit[]>([]);
  readonly brokenSkillRefs = input<string[]>([]);

  protected addBusinessRule(): void {
    this.businessRules.update((list) => [...list, '']);
  }

  protected removeBusinessRule(index: number): void {
    this.businessRules.update((list) => list.filter((_, i) => i !== index));
  }

  protected updateBusinessRule(index: number, value: string): void {
    this.businessRules.update((list) => list.map((r, i) => (i === index ? value : r)));
  }

  protected addEscalationRule(): void {
    this.escalationRules.update((list) => [
      ...list,
      { name: '', trigger: 'human_request', keywords: [], requiredSkillIds: [] },
    ]);
  }

  protected removeEscalationRule(index: number): void {
    this.escalationRules.update((list) => list.filter((_, i) => i !== index));
  }

  protected updateEscalationName(index: number, name: string): void {
    this.escalationRules.update((list) => list.map((r, i) => (i === index ? { ...r, name } : r)));
  }

  protected updateEscalationTrigger(
    index: number,
    trigger: 'human_request' | 'topic_keywords',
  ): void {
    this.escalationRules.update((list) =>
      list.map((r, i) => (i === index ? { ...r, trigger } : r)),
    );
  }

  protected updateEscalationKeywords(index: number, raw: string): void {
    const keywords = raw
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    this.escalationRules.update((list) =>
      list.map((r, i) => (i === index ? { ...r, keywords } : r)),
    );
  }

  protected updateEscalationSkills(index: number, raw: string): void {
    const requiredSkillIds = raw
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    this.escalationRules.update((list) =>
      list.map((r, i) => (i === index ? { ...r, requiredSkillIds } : r)),
    );
  }
}
