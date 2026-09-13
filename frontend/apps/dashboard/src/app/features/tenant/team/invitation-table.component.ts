import { DatePipe } from '@angular/common';
import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { TenantInvitation } from '../../../core/api/tenant-api.models';
import { INVITATION_STATUS_TONES } from '../../../core/ui/status-badge-config';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';

@Component({
  selector: 'app-invitation-table',
  imports: [ButtonComponent, DataTableComponent, DatePipe, StatusBadgeComponent],
  templateUrl: './invitation-table.component.html',
  styles: `
    h3 {
      margin: 0 0 var(--app-space-2);
      font-size: var(--app-font-sm);
    }
    .role {
      text-transform: capitalize;
      color: var(--app-text-2);
    }
    caption {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip: rect(0 0 0 0);
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class InvitationTableComponent {
  readonly title = input.required<string>();
  readonly headingId = input.required<string>();
  readonly invitations = input.required<TenantInvitation[]>();
  readonly revocableIds = input<string[]>([]);
  readonly revokingId = input<string | null>(null);
  readonly showActions = input(false);
  readonly revoke = output<string>();

  protected invitationTone(invitation: TenantInvitation) {
    return INVITATION_STATUS_TONES[invitation.status];
  }

  protected deliveryLabel(invitation: TenantInvitation): string {
    switch (invitation.emailDeliveryStatus) {
      case 'queued':
        return 'Email queued';
      case 'sent':
        return 'Email sent';
      case 'failed':
        return 'Email failed';
      default:
        return 'Email not configured';
    }
  }
}
