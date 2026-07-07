import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

@Component({
  selector: 'app-sparkline',
  template: `
    <svg aria-hidden="true" viewBox="0 0 100 32" preserveAspectRatio="none">
      <polyline [attr.points]="polyline()" [attr.stroke]="stroke()" />
    </svg>
  `,
  styles: [
    `
      :host {
        display: block;
        width: 100%;
        height: 32px;
      }
      svg {
        display: block;
        width: 100%;
        height: 100%;
      }
      polyline {
        fill: none;
        stroke-width: 2.4;
        stroke-linecap: round;
        stroke-linejoin: round;
        vector-effect: non-scaling-stroke;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SparklineComponent {
  readonly points = input.required<readonly number[]>();
  readonly colorToken = input<'accent' | 'green' | 'red' | 'amber'>('accent');

  protected readonly stroke = computed(() => `var(--app-${this.colorToken()})`);
  protected readonly polyline = computed(() => {
    const points = this.points();
    if (points.length === 0) {
      return '';
    }

    const min = Math.min(...points);
    const max = Math.max(...points);
    const span = max - min || 1;
    const lastIndex = Math.max(points.length - 1, 1);

    return points
      .map((point, index) => {
        const x = (index / lastIndex) * 100;
        const y = 28 - ((point - min) / span) * 24;
        return `${x.toFixed(2)},${y.toFixed(2)}`;
      })
      .join(' ');
  });
}
