import { ChangeDetectionStrategy, Component, input } from '@angular/core';

export interface BreakdownBarItem {
  label: string;
  count: number;
  share: number;
}

@Component({
  selector: 'app-breakdown-bars',
  template: `
    <ul [attr.aria-label]="ariaLabel()">
      @for (item of items(); track item.label) {
        <li>
          <span class="label">{{ item.label }}</span>
          <span class="value">{{ item.count }} ({{ formatPct(item.share) }})</span>
          <div class="bar-track">
            <div class="bar-fill" [style.width.%]="item.share * 100"></div>
          </div>
        </li>
      }
    </ul>
  `,
  styles: [
    `
      ul {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: var(--app-space-3);
      }
      li {
        display: grid;
        grid-template-columns: 1fr auto;
        align-items: center;
        gap: var(--app-space-2);
      }
      .label {
        color: var(--app-text);
        font-size: var(--app-font-sm);
      }
      .value {
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        text-align: right;
        white-space: nowrap;
      }
      .bar-track {
        grid-column: 1 / -1;
        height: 8px;
        background: var(--app-border);
        border-radius: var(--app-radius-sm);
        overflow: hidden;
      }
      .bar-fill {
        height: 100%;
        background: var(--app-chart-1);
        border-radius: var(--app-radius-sm);
        transition: width 0.3s ease;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BreakdownBarsComponent {
  readonly items = input.required<readonly BreakdownBarItem[]>();
  readonly ariaLabel = input('Channel breakdown');

  protected formatPct(share: number): string {
    return `${(share * 100).toFixed(1)}%`;
  }
}
