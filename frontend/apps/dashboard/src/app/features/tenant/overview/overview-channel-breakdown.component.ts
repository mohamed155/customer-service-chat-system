import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { LocaleNumberPipe } from '../../../core/i18n/format';
import { ChannelBreakdownFixture } from '../../../shared/fixtures/fixture.models';
import { ChannelBadgeComponent } from '../../../shared/components/channel-badge/channel-badge.component';

@Component({
  selector: 'app-overview-channel-breakdown',
  imports: [ChannelBadgeComponent, LocaleNumberPipe],
  templateUrl: './overview-channel-breakdown.component.html',
  styles: [
    `
      :host {
        display: grid;
        grid-template-columns: 150px 1fr;
        gap: var(--app-space-5);
        align-items: center;
      }
      .donut {
        position: relative;
        width: 150px;
        height: 150px;
      }
      svg {
        width: 100%;
        height: 100%;
        transform: rotate(-90deg);
      }
      circle {
        fill: none;
        stroke-width: 16;
        stroke-linecap: round;
      }
      strong {
        position: absolute;
        inset: 0;
        display: grid;
        place-items: center;
        color: var(--app-text);
        font-size: var(--app-font-xl);
      }
      .list {
        display: grid;
        gap: var(--app-space-3);
      }
      .list div {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-3);
        color: var(--app-text-2);
        font-weight: 650;
      }
      @media (max-width: 768px) {
        :host {
          grid-template-columns: 1fr;
          justify-items: center;
        }
        .list {
          width: 100%;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class OverviewChannelBreakdownComponent {
  readonly breakdown = input.required<readonly ChannelBreakdownFixture[]>();
  protected readonly percentFormat = { style: 'percent', maximumFractionDigits: 1 } as const;
  protected readonly colors = [
    'var(--app-accent)',
    'var(--app-green)',
    'var(--app-amber)',
    'var(--app-red)',
  ] as const;

  protected dash(item: ChannelBreakdownFixture): string {
    const circumference = 2 * Math.PI * 42;
    const length = (item.percentage / 100) * circumference;
    return `${length} ${circumference - length}`;
  }

  protected offset(index: number): number {
    const circumference = 2 * Math.PI * 42;
    return -this.breakdown()
      .slice(0, index)
      .reduce((sum, item) => sum + (item.percentage / 100) * circumference, 0);
  }
}
