import { TranslocoPipe } from '@jsverse/transloco';
import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

export interface TrendSeries {
  id: string;
  label: string;
  color: 'chart-1' | 'chart-2';
  points: readonly (number | null)[];
}

@Component({
  imports: [TranslocoPipe],
  selector: 'app-trend-chart',
  standalone: true,
  styleUrls: ['./trend-chart.component.css'],
  templateUrl: './trend-chart.component.html',
  styles: [
    `
      :host {
        display: block;
        width: 100%;
      }
      svg {
        display: block;
        width: 100%;
        height: auto;
        overflow: visible;
      }
      polyline {
        stroke-linejoin: round;
        stroke-linecap: round;
      }
      .legend {
        display: flex;
        flex-wrap: wrap;
        gap: var(--app-space-3);
        margin: var(--app-space-3) 0 0;
        padding: 0;
        list-style: none;
      }
      .legend li {
        display: flex;
        align-items: center;
        gap: 6px;
      }
      .swatch {
        display: inline-block;
        width: 8px;
        height: 8px;
        border-radius: 2px;
        flex-shrink: 0;
      }
      .label {
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TrendChartComponent {
  readonly series = input.required<readonly TrendSeries[]>();
  readonly labels = input.required<readonly string[]>();
  /** The heading of the data column; the default is "Value". */
  readonly valueLabel = input<string | null>(null);

  protected readonly viewBox = computed(() => '0 0 100 40');

  protected readonly seriesNames = computed(() =>
    this.series()
      .map((s) => s.label)
      .join(', '),
  );

  protected readonly sharedScale = computed(() => {
    let min = Infinity;
    let max = -Infinity;
    for (const s of this.series()) {
      for (const p of s.points) {
        if (p !== null) {
          if (p < min) min = p;
          if (p > max) max = p;
        }
      }
    }
    if (!isFinite(min)) {
      min = 0;
      max = 0;
    }
    const span = max - min || 1;
    return { min, max, span };
  });

  protected readonly segments = computed(() => {
    const scale = this.sharedScale();
    const allSeries = this.series();
    const maxIndex = Math.max(...allSeries.map((s) => s.points.length), 1) - 1;
    const divisor = Math.max(maxIndex, 1);

    const result: { points: string; stroke: string }[] = [];
    for (const s of allSeries) {
      let current: string[] = [];
      const stroke = `var(--app-${s.color})`;
      for (let i = 0; i < s.points.length; i++) {
        const p = s.points[i];
        if (p === null) {
          if (current.length > 0) {
            result.push({ points: current.join(' '), stroke });
            current = [];
          }
        } else {
          const x = (i / divisor) * 100;
          const y = 36 - ((p - scale.min) / scale.span) * 32;
          current.push(`${x.toFixed(2)},${y.toFixed(2)}`);
        }
      }
      if (current.length > 0) {
        result.push({ points: current.join(' '), stroke });
      }
    }
    return result;
  });

  protected readonly tooltipRects = computed(() => {
    const allSeries = this.series();
    const allLabels = this.labels();
    const count = Math.min(...allSeries.map((s) => s.points.length), allLabels.length);
    const maxIndex = Math.max(count - 1, 1);
    const w = 100 / count;

    return Array.from({ length: count }, (_, i) => {
      const x = Math.max(0, (i / maxIndex) * 100 - w / 2);
      const parts = [allLabels[i] ?? ''];
      for (const s of allSeries) {
        const val = s.points[i];
        parts.push(`${s.label}: ${val !== null && val !== undefined ? val : '\u2014'}`);
      }
      return { x, width: w, label: parts.join(' \u2014 ') };
    });
  });

  protected formatPoint(value: number | null | undefined): string {
    if (value === null || value === undefined) return '\u2014';
    return String(value);
  }
}
