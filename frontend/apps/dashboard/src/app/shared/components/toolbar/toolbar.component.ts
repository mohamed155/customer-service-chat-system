import { ChangeDetectionStrategy, Component } from '@angular/core';

@Component({
  selector: 'app-toolbar',
  template: `
    <div class="start"><ng-content select="[toolbar-start]" /></div>
    <div class="end"><ng-content select="[toolbar-end]" /></div>
  `,
  styles: [
    `
      :host {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-3);
        flex-wrap: wrap;
        padding: var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
      }
      .start,
      .end {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        min-width: 0;
      }
      .start {
        flex: 1 1 260px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolbarComponent {}
