import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { PromptVariable } from '../../../../core/api/ai-agent.models';

@Component({
  selector: 'app-variables-panel',
  standalone: true,
  templateUrl: './variables-panel.component.html',
  styles: [
    `
      .variables-panel {
        padding: var(--app-space-4);
      }
      .panel-title {
        margin: 0 0 var(--app-space-2);
        font-size: var(--app-font-sm);
        font-weight: 650;
        color: var(--app-text);
      }
      .panel-hint {
        margin: 0 0 var(--app-space-3);
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
      }
      .variable-list {
        display: flex;
        flex-wrap: wrap;
        gap: var(--app-space-2);
      }
      .variable-chip {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-2);
        padding: var(--app-space-1) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        cursor: pointer;
        font: inherit;
        font-size: var(--app-font-xs);
        transition:
          border-color 0.15s,
          background 0.15s;
      }
      .variable-chip:hover {
        border-color: var(--app-accent);
        background: var(--app-accent-soft);
      }
      .var-name {
        font-family: monospace;
        color: var(--app-accent);
        font-weight: 600;
      }
      .var-sample {
        color: var(--app-text-3);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class VariablesPanelComponent {
  readonly variables = input<PromptVariable[]>([]);
  readonly insertVariable = output<string>();
}
