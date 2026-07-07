import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { TrendSeriesFixture } from '../../../shared/fixtures/fixture.models';

@Component({
  selector: 'app-overview-trend-chart',
  template: `
    <svg viewBox="0 0 640 260" role="img" aria-label="Conversation trend">
      <g class="grid">
        @for (line of gridLines; track line) {
          <line x1="0" [attr.y1]="line" x2="640" [attr.y2]="line" />
        }
      </g>
      @for (item of series(); track item.id) {
        <path [attr.d]="path(item.points)" [attr.stroke]="'var(--app-' + item.colorToken + ')'" />
      }
    </svg>
    <div class="legend">
      @for (item of series(); track item.id) {
        <span
          ><i [style.background]="'var(--app-' + item.colorToken + ')'"></i>{{ item.label }}</span
        >
      }
    </div>
  `,
  styles: [
    `
      :host {
        display: grid;
        gap: var(--app-space-4);
      }
      svg {
        width: 100%;
        height: auto;
        min-height: 240px;
      }
      .grid line {
        stroke: var(--app-border);
      }
      path {
        fill: none;
        stroke-width: 3;
        stroke-linecap: round;
        stroke-linejoin: round;
      }
      .legend {
        display: flex;
        gap: var(--app-space-4);
        flex-wrap: wrap;
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
      }
      .legend span {
        display: inline-flex;
        align-items: center;
        gap: 7px;
      }
      i {
        width: 8px;
        height: 8px;
        border-radius: 999px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class OverviewTrendChartComponent {
  readonly series = input.required<readonly TrendSeriesFixture[]>();
  protected readonly gridLines = [40, 90, 140, 190, 240] as const;

  protected path(points: readonly number[]): string {
    const allPoints = this.series().flatMap((item) => [...item.points]);
    const min = Math.min(...allPoints);
    const max = Math.max(...allPoints);
    const span = max - min || 1;
    const lastIndex = Math.max(points.length - 1, 1);

    return points
      .map((point, index) => {
        const x = (index / lastIndex) * 640;
        const y = 230 - ((point - min) / span) * 200;
        return `${index === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`;
      })
      .join(' ');
  }
}
