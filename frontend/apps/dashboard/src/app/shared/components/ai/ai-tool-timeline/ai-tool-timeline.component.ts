import { ChangeDetectionStrategy, Component, input } from '@angular/core';

export interface AiToolTimelineStep {
  readonly label: string;
  readonly detail?: string;
}

@Component({
  selector: 'app-ai-tool-timeline',
  template: `
    @for (step of steps(); track step.label) {
      <div class="step">
        <span></span>
        <div>
          <strong>{{ step.label }}</strong>
          @if (step.detail) {
            <p>{{ step.detail }}</p>
          }
        </div>
      </div>
    }
  `,
  styles: [
    `
      :host {
        display: grid;
        gap: var(--app-space-3);
      }
      .step {
        display: grid;
        grid-template-columns: 18px 1fr;
        gap: var(--app-space-3);
        color: var(--app-text);
      }
      .step > span {
        width: 10px;
        height: 10px;
        margin-top: 5px;
        border-radius: 999px;
        background: var(--app-accent);
        box-shadow: 0 0 0 4px var(--app-accent-soft);
      }
      strong {
        display: block;
        font-size: var(--app-font-sm);
      }
      p {
        margin: 3px 0 0;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AiToolTimelineComponent {
  readonly steps = input.required<readonly AiToolTimelineStep[]>();
}
