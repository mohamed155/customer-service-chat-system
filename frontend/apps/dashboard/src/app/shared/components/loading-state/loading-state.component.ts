import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'app-loading-state',
  template: `<span aria-hidden="true"></span>
    <p>{{ label() }}</p>`,
  styles: [
    `
      :host {
        display: grid;
        justify-items: center;
        gap: var(--app-space-3);
        padding: var(--app-space-8);
        color: var(--app-text-2);
      }
      span {
        width: 24px;
        height: 24px;
        border-radius: 999px;
        border: 3px solid var(--app-border);
        border-top-color: var(--app-accent);
        animation: spin 800ms linear infinite;
      }
      p {
        margin: 0;
        font-size: var(--app-font-sm);
      }
      @keyframes spin {
        to {
          transform: rotate(360deg);
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LoadingStateComponent {
  readonly label = input('Loading');
}
