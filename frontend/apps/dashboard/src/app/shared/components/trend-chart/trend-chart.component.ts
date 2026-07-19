import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

export interface TrendSeries {
  id: string;
  label: string;
  color: 'chart-1' | 'chart-2';
  points: readonly (number | null)[];
}

@Component({
  selector: 'app-trend-chart',
  standalone: true,
  styleUrls: ['./trend-chart.component.css'],
  template: `
    <svg
      [attr.viewBox]="viewBox()"
      preserveAspectRatio="none"
      role="img"
      [attr.aria-label]="ariaLabel()"
    >
      @for (segment of segments(); track $index) {
        <polyline
          [attr.points]="segment.points"
          [attr.stroke]="segment.stroke"
          stroke-width="2"
          fill="none"
          vector-effect="non-scaling-stroke"
        />
      }
      @for (rect of tooltipRects(); track $index) {
        <rect [attr.x]="rect.x" [attr.width]="rect.width" y="0" height="40" fill="transparent">
          <title>{{ rect.label }}</title>
        </rect>
      }
    </svg>
    @if (series().length > 1) {
      <ul class="legend" role="list">
        @for (s of series(); track s.id) {
          <li>
            <span class="swatch" [style.background]="'var(--app-' + s.color + ')'"></span>
            <span class="label">{{ s.label }}</span>
          </li>
        }
      </ul>
    }
    <table class="sr-only" role="table">
      <caption>
        Chart data
      </caption>
      <thead>
        <tr>
          <th scope="col">{{ valueLabel() }}</th>
          @for (s of series(); track s.id) {
            <th scope="col">{{ s.label }}</th>
          }
        </tr>
      </thead>
      <tbody>
        @for (label of labels(); track $index; let i = $index) {
          <tr>
            <th scope="row">{{ label }}</th>
            @for (s of series(); track s.id) {
              <td>{{ formatPoint(s.points[i]) }}</td>
            }
          </tr>
        }
      </tbody>
    </table>
  `,
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
  readonly valueLabel = input('Value');

  protected readonly viewBox = computed(() => '0 0 100 40');

  protected readonly ariaLabel = computed(() => {
    const names = this.series()
      .map((s) => s.label)
      .join(', ');
    return `Chart: ${names}`;
  });

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
