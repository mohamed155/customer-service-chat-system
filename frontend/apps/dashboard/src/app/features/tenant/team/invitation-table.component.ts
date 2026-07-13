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
  template: `
    <section [attr.aria-labelledby]="headingId()">
      <h3 [id]="headingId()">{{ title() }}</h3>
      <ng-content />
      <app-data-table>
        <table>
          <caption>
            {{
              title()
            }}
          </caption>
          <thead>
            <tr>
              <th scope="col">Email</th>
              <th scope="col">Role</th>
              <th scope="col">Status</th>
              <th scope="col">Email delivery</th>
              <th scope="col">Invited</th>
              @if (showActions()) {
                <th scope="col">Actions</th>
              }
            </tr>
          </thead>
          <tbody>
            @for (invitation of invitations(); track invitation.id) {
              <tr>
                <td>
                  <strong>{{ invitation.email }}</strong>
                </td>
                <td class="role">{{ invitation.role }}</td>
                <td>
                  <app-status-badge
                    [status]="invitation.status"
                    [tone]="invitationTone(invitation)"
                  />
                </td>
                <td>{{ deliveryLabel(invitation) }}</td>
                <td class="muted">{{ invitation.createdAt | date: 'mediumDate' }}</td>
                @if (showActions()) {
                  <td>
                    @if (revocableIds().includes(invitation.id)) {
                      <app-button
                        variant="danger"
                        [ariaLabel]="'Revoke invitation for ' + invitation.email"
                        [disabled]="revokingId() === invitation.id"
                        (pressed)="revoke.emit(invitation.id)"
                        >{{ revokingId() === invitation.id ? 'Revoking…' : 'Revoke' }}</app-button
                      >
                    }
                  </td>
                }
              </tr>
            }
          </tbody>
        </table>
      </app-data-table>
    </section>
  `,
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
