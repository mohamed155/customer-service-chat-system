import { ChangeDetectionStrategy, Component, computed, input, model } from '@angular/core';

@Component({
  selector: 'app-prompt-editor-shell',
  template: `
    <div class="gutter">
      @for (line of lines(); track line) {
        <span>{{ line }}</span>
      }
    </div>
    <textarea
      [attr.aria-label]="label()"
      [value]="value()"
      (input)="updateValue($event)"
      spellcheck="false"
    ></textarea>
  `,
  styles: [
    `
      :host {
        display: grid;
        grid-template-columns: 44px 1fr;
        min-height: 280px;
        overflow: hidden;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel-2);
        font-family: var(--app-font-mono);
      }
      .gutter {
        display: grid;
        align-content: start;
        gap: 0;
        padding: var(--app-space-3) 0;
        background: var(--app-panel-3);
        color: var(--app-text-3);
        text-align: right;
      }
      .gutter span {
        padding-right: var(--app-space-3);
        font-size: var(--app-font-xs);
        line-height: 22px;
      }
      textarea {
        resize: vertical;
        min-height: 280px;
        border: 0;
        outline: 0;
        padding: var(--app-space-3);
        background: transparent;
        color: var(--app-text);
        font: 400 var(--app-font-sm) / 22px var(--app-font-mono);
      }
      textarea:focus {
        box-shadow: inset 0 0 0 3px var(--app-accent-soft);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PromptEditorShellComponent {
  readonly label = input('System prompt');
  readonly value = model('You are Helix, a concise AI support assistant.');
  protected readonly lines = computed(() =>
    Array.from({ length: Math.max(this.value().split('\n').length, 10) }, (_, index) => index + 1),
  );

  protected updateValue(event: Event): void {
    this.value.set((event.target as HTMLTextAreaElement).value);
  }
}
