import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { Escalation } from '../../../core/api/tenant-api.models';

@Component({
  selector: 'app-escalation-banner',
  template: `
    @if (escalation(); as esc) {
      <div class="banner">
        <span class="label">Escalated · {{ routingReasonLabel(esc.routing?.reason) }}</span>
        @if (esc.status === 'assigned' && esc.routing) {
          <span class="detail">Assigned to agent</span>
        }
        @if (esc.status === 'queued') {
          <span class="detail">Waiting in queue</span>
        }
      </div>
    }
  `,
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
export class EscalationBannerComponent {
  readonly escalation = input<Escalation | null>(null);

  routingReasonLabel(reason?: string): string {
    const labels: Record<string, string> = {
      skill_match: 'Skill match',
      load_fallback: 'Load fallback',
      manual_claim: 'Manual claim',
      queue_auto: 'Queue auto',
      manual_reassignment: 'Manual reassignment',
    };
    return reason ? (labels[reason] ?? reason) : 'Unknown';
  }
}
