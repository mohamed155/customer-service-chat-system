import { ChangeDetectionStrategy, Component, input, model } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

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
  providers: [provideTranslocoScope('ai-agent')],
  imports: [FormsModule, ButtonComponent, InlineAlertComponent, TranslocoPipe],
  templateUrl: './rules-editor.component.html',
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
