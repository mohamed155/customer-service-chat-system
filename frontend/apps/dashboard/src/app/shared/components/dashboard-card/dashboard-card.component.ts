import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'app-dashboard-card',
  host: { '[class.padding-none]': "padding() === 'none'" },
  template: `
    <div class="card-header">
      <ng-content select="[card-header]" />
    </div>
    <div class="card-body">
      <ng-content />
    </div>
    <div class="card-footer">
      <ng-content select="[card-footer]" />
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
        padding: var(--app-space-5);
        background: var(--app-panel);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        box-shadow: var(--app-shadow);
      }
      :host.padding-none {
        padding: 0;
      }
      .card-header:empty,
      .card-footer:empty {
        display: none;
      }
      .card-header {
        margin-bottom: var(--app-space-4);
      }
      .card-footer {
        margin-top: var(--app-space-4);
        padding-top: var(--app-space-4);
        border-top: 1px solid var(--app-border);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DashboardCardComponent {
  readonly padding = input<'md' | 'none'>('md');
}
