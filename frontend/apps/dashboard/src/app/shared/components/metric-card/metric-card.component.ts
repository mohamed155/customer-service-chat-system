import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { MetricFixture } from '../../fixtures/fixture.models';
import { DashboardCardComponent } from '../dashboard-card/dashboard-card.component';
import { SparklineComponent } from '../sparkline/sparkline.component';

@Component({
  selector: 'app-metric-card',
  imports: [DashboardCardComponent, SparklineComponent, TuiIcon],
  template: `
    <app-dashboard-card>
      <div class="metric-head">
        <span class="icon"><tui-icon [icon]="metric().icon" /></span>
        <span [class]="deltaClass()">{{ metric().delta }}</span>
      </div>
      <p>{{ metric().label }}</p>
      <strong>{{ metric().value }}</strong>
      <app-sparkline [points]="metric().trend" [colorToken]="sparkColor()" />
    </app-dashboard-card>
  `,
  styles: [
    `
      :host {
        display: block;
      }
      .metric-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-3);
        margin-bottom: var(--app-space-4);
      }
      .icon {
        width: 34px;
        height: 34px;
        display: grid;
        place-items: center;
        border-radius: var(--app-radius-md);
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      .delta {
        display: inline-flex;
        align-items: center;
        min-height: 22px;
        padding: 0 var(--app-space-2);
        border-radius: 999px;
        font-size: var(--app-font-xs);
        font-weight: 650;
      }
      .positive {
        background: var(--app-green-soft);
        color: var(--app-green);
      }
      .negative {
        background: var(--app-red-soft);
        color: var(--app-red);
      }
      p {
        margin: 0 0 6px;
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
      }
      strong {
        display: block;
        margin-bottom: var(--app-space-3);
        color: var(--app-text);
        font-size: var(--app-font-2xl);
        font-weight: 700;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class MetricCardComponent {
  readonly metric = input.required<MetricFixture>();

  protected readonly deltaClass = computed(() =>
    this.metric().deltaPositive ? 'delta positive' : 'delta negative',
  );
  protected readonly sparkColor = computed(() => (this.metric().deltaPositive ? 'green' : 'red'));
}
