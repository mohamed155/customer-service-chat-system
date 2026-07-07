import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { ChannelBadgeComponent } from '../../../shared/components/channel-badge/channel-badge.component';
import { SentimentBadgeComponent } from '../../../shared/components/sentiment-badge/sentiment-badge.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { CustomerFixture } from '../../../shared/fixtures/fixture.models';

@Component({
  selector: 'app-customer-panel',
  imports: [AvatarComponent, ChannelBadgeComponent, SentimentBadgeComponent, StatusBadgeComponent],
  template: `
    @if (customer(); as profile) {
      <section class="profile">
        <app-avatar [initials]="profile.avatarInitials" size="lg" />
        <h2>{{ profile.name }}</h2>
        <p>{{ profile.email }}</p>
        <div class="badges">
          <app-channel-badge [channel]="profile.channel" />
          <app-status-badge [status]="profile.tier" tone="accent" />
          <app-sentiment-badge [sentiment]="profile.sentiment" />
        </div>
      </section>

      <dl>
        <div>
          <dt>Customer since</dt>
          <dd>{{ profile.since }}</dd>
        </div>
        <div>
          <dt>Orders</dt>
          <dd>{{ profile.orders }}</dd>
        </div>
        <div>
          <dt>Total spend</dt>
          <dd>{{ profile.totalSpend }}</dd>
        </div>
        <div>
          <dt>CSAT</dt>
          <dd>{{ profile.csat }}%</dd>
        </div>
        <div>
          <dt>Interactions</dt>
          <dd>{{ profile.interactions }}</dd>
        </div>
      </dl>

      <section>
        <h3>Recent activity</h3>
        @for (activity of profile.recentActivity; track activity.label) {
          <article>
            <strong>{{ activity.label }}</strong>
            <span>{{ activity.at }}</span>
          </article>
        }
      </section>
    }
  `,
  styles: [
    `
      :host {
        min-height: 0;
        display: block;
        overflow-y: auto;
        padding: var(--app-space-4);
        border-left: 1px solid var(--app-border);
        background: var(--app-panel);
      }
      .profile {
        display: grid;
        justify-items: center;
        gap: var(--app-space-2);
        padding-bottom: var(--app-space-4);
        border-bottom: 1px solid var(--app-border);
        text-align: center;
      }
      h2,
      h3 {
        margin: 0;
        color: var(--app-text);
        font-size: var(--app-font-lg);
      }
      .profile p {
        margin: 0;
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
      }
      .badges {
        display: flex;
        gap: var(--app-space-2);
        flex-wrap: wrap;
        justify-content: center;
      }
      dl {
        display: grid;
        gap: var(--app-space-2);
        margin: var(--app-space-4) 0;
      }
      dl div {
        display: flex;
        justify-content: space-between;
        gap: var(--app-space-3);
      }
      dt {
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
      }
      dd {
        margin: 0;
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 650;
      }
      section:last-child {
        display: grid;
        gap: var(--app-space-3);
      }
      article {
        padding: var(--app-space-3);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
      }
      article strong,
      article span {
        display: block;
        font-size: var(--app-font-sm);
      }
      article span {
        margin-top: 3px;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      @media (max-width: 1024px) {
        :host {
          display: none;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CustomerPanelComponent {
  readonly customer = input<CustomerFixture | null>(null);
}
