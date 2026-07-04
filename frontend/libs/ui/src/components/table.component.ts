import { Component } from '@angular/core';
@Component({
  selector: 'hx-table',
  standalone: true,
  template: `<table class="hx-table" role="table">
    <ng-content />
  </table>`,
  styles: [
    `
      .hx-table {
        background: var(--panel);
        border: 1px solid var(--border);
        border-collapse: collapse;
        color: var(--text);
        width: 100%;
      }
      .hx-table :where(th, td) {
        border-bottom: 1px solid var(--border);
        padding: 8px;
      }
      .hx-table__row {
        background: var(--panel);
      }
      .hx-table__cell {
        color: var(--text);
      }
    `,
  ],
})
export class TableComponent {}
