import { ChangeDetectionStrategy, Component } from '@angular/core';

@Component({
  selector: 'app-data-table',
  template: `<div class="table-wrap"><ng-content /></div>`,
  styles: [
    `
      :host {
        display: block;
        overflow: hidden;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
        box-shadow: var(--app-shadow);
      }
      .table-wrap {
        width: 100%;
        overflow-x: auto;
      }
      :host ::ng-deep table {
        width: 100%;
        border-collapse: collapse;
        min-width: 720px;
      }
      :host ::ng-deep th,
      :host ::ng-deep td {
        padding: 12px 14px;
        border-bottom: 1px solid var(--app-border);
        text-align: left;
        font-size: var(--app-font-sm);
      }
      :host ::ng-deep th {
        color: var(--app-text-3);
        font-weight: 650;
        text-transform: uppercase;
        font-size: var(--app-font-xs);
      }
      :host ::ng-deep tr:last-child td {
        border-bottom: 0;
      }
      :host ::ng-deep .muted {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DataTableComponent {}
