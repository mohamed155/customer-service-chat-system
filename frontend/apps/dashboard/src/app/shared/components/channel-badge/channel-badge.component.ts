import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { Channel as FixtureChannel } from '../../fixtures/fixture.models';
import { TranslocoPipe } from '@jsverse/transloco';
import { marker } from '../../../core/i18n/marker';

export type ChannelBadgeChannel =
  FixtureChannel | 'email' | 'phone' | 'web_chat' | 'whatsapp' | 'telegram' | 'widget';

const CHANNEL_LABELS: Record<ChannelBadgeChannel, string> = {
  web: marker('channels.web'),
  web_chat: marker('channels.web_chat'),
  whatsapp: marker('channels.whatsapp'),
  telegram: marker('channels.telegram'),
  'mobile-sdk': marker('channels.mobile_sdk'),
  email: marker('channels.email'),
  phone: marker('channels.phone'),
  widget: marker('channels.widget'),
};

const CHANNEL_ICONS: Partial<Record<ChannelBadgeChannel, string>> = {
  email: '@tui.mail',
  phone: '@tui.phone',
};

@Component({
  selector: 'app-channel-badge',
  imports: [TuiIcon, TranslocoPipe],
  templateUrl: './channel-badge.component.html',
  styles: [
    `
      :host {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        min-height: 22px;
        padding: 0 var(--app-space-2);
        border-radius: 999px;
        background: var(--app-panel-2);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        font-weight: 600;
        white-space: nowrap;
      }
      .dot {
        width: 6px;
        height: 6px;
        border-radius: 999px;
        background: var(--app-accent);
      }
      tui-icon {
        width: 14px;
        height: 14px;
        color: var(--app-accent);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ChannelBadgeComponent {
  readonly channel = input.required<ChannelBadgeChannel>();
  protected readonly label = computed(() => CHANNEL_LABELS[this.channel()]);
  protected readonly icon = computed(() => CHANNEL_ICONS[this.channel()]);
}
