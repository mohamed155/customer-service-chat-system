import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { Escalation } from '../../../core/api/tenant-api.models';
import { TranslocoPipe } from '@jsverse/transloco';
import { marker } from '../../../core/i18n/marker';
import { ownValue } from '../../../core/i18n/own-value';

const ROUTING_REASON_KEYS: Readonly<Record<string, string>> = {
  skill_match: marker('escalationRouting.reason.skill_match'),
  load_fallback: marker('escalationRouting.reason.load_fallback'),
  manual_claim: marker('escalationRouting.reason.manual_claim'),
  queue_auto: marker('escalationRouting.reason.queue_auto'),
  manual_reassignment: marker('escalationRouting.reason.manual_reassignment'),
};

@Component({
  imports: [TranslocoPipe],
  selector: 'app-escalation-routing-banner',
  templateUrl: './escalation-routing-banner.component.html',
  styles: [
    `
      .banner {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 8px 16px;
        border-radius: var(--app-radius-md);
        background: var(--app-warning-bg, #fff3cd);
        border: 1px solid var(--app-warning-border, #ffc107);
        font-size: var(--app-font-sm);
      }
      .label {
        font-weight: 600;
        color: var(--app-text);
      }
      .detail {
        color: var(--app-text-2);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class EscalationRoutingBannerComponent {
  readonly escalation = input<Escalation | null>(null);

  /** Catalog key of the routing reason; a code this app does not know shows a generic text. */
  routingReasonLabel(reason?: string): string {
    if (!reason) return marker('escalationRouting.reason.unknown');
    return ownValue(ROUTING_REASON_KEYS, reason) ?? marker('escalationRouting.reason.other');
  }
}
