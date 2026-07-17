import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { PromptVariable } from '../../../../core/api/ai-agent.models';
import { renderPreview } from './prompt-lang';

@Component({
  selector: 'app-preview-panel',
  standalone: true,
  template: `
    <div class="preview-panel">
      <h3 class="panel-title">Preview</h3>
      <div class="preview-content">
        @if (preview().text.length === 0) {
          <span class="placeholder-text">Preview will appear here as you type.</span>
        } @else {
          <span class="preview-text">{{ preview().text }}</span>
        }
      </div>
      @if (preview().errorSpans.length > 0) {
        <div class="error-spans">
          @for (span of preview().errorSpans; track $index) {
            <span class="error-chip">
              {{ span.reason === 'malformed' ? 'Malformed' : 'Unknown' }} placeholder at position
              {{ span.start }}
            </span>
          }
        </div>
      }
    </div>
  `,
  styles: [
    `
      .preview-panel {
        padding: var(--app-space-4);
      }
      .panel-title {
        margin: 0 0 var(--app-space-3);
        font-size: var(--app-font-sm);
        font-weight: 650;
        color: var(--app-text);
      }
      .preview-content {
        padding: var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-bg);
        min-height: 60px;
        font-size: var(--app-font-sm);
        line-height: 1.6;
        white-space: pre-wrap;
        word-break: break-word;
      }
      .preview-text {
        color: var(--app-text);
      }
      .placeholder-text {
        color: var(--app-text-3);
        font-style: italic;
      }
      .error-spans {
        display: flex;
        flex-wrap: wrap;
        gap: var(--app-space-2);
        margin-top: var(--app-space-3);
      }
      .error-chip {
        display: inline-block;
        padding: var(--app-space-1) var(--app-space-2);
        border-radius: var(--app-radius-sm);
        background: var(--app-danger-soft, rgba(220, 38, 38, 0.1));
        color: var(--app-danger, #dc2626);
        font-size: var(--app-font-xs);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PreviewPanelComponent {
  readonly content = input<string>('');
  readonly variables = input<PromptVariable[] | null>(null);

  protected readonly preview = computed(() => {
    const content = this.content();
    const vars = this.variables();
    const samples: Record<string, string> = {};
    if (vars) {
      for (const v of vars) {
        samples[v.name] = v.sample;
      }
    }
    return renderPreview(content, samples);
  });
}
