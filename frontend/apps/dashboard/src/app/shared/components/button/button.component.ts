import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

@Component({
  selector: 'app-button',
  template: `
    <button
      [attr.type]="type()"
      [class]="variant()"
      [class.sm]="size() === 'sm'"
      [disabled]="disabled()"
      [attr.aria-label]="ariaLabel()"
      (click)="pressed.emit($event)"
    >
      <ng-content />
    </button>
  `,
  styles: [
    `
      :host {
        display: inline-flex;
      }
      button {
        min-height: 38px;
        padding: 0 var(--app-space-4);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        cursor: pointer;
        font: inherit;
        font-size: var(--app-font-sm);
        font-weight: 600;
        transition:
          background var(--app-transition-fast),
          border-color var(--app-transition-fast),
          color var(--app-transition-fast),
          opacity var(--app-transition-fast);
      }
      button.primary {
        border-color: transparent;
        background: var(--app-accent);
        color: var(--app-accent-on, white);
      }
      button.danger {
        background: transparent;
        color: var(--app-red);
      }
      button.ghost {
        background: transparent;
      }
      button:hover:not(:disabled) {
        background: var(--app-panel-2);
      }
      button.primary:hover:not(:disabled) {
        background: var(--app-accent);
        opacity: 0.92;
      }
      button.danger:hover:not(:disabled) {
        background: color-mix(in srgb, var(--app-red) 10%, transparent);
      }
      button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-color: var(--app-accent);
      }
      button.sm {
        min-height: 28px;
        padding: 0 var(--app-space-2);
        font-size: var(--app-font-xs);
      }
      button:disabled {
        opacity: 0.6;
        cursor: default;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ButtonComponent {
  readonly variant = input<'primary' | 'secondary' | 'danger' | 'ghost'>('secondary');
  readonly size = input<'sm' | 'md'>('md');
  readonly type = input<'button' | 'submit' | 'reset'>('button');
  readonly disabled = input(false);
  readonly ariaLabel = input<string>();
  readonly pressed = output<MouseEvent>();
}
