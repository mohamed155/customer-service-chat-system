import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { PAGE_PERMISSIONS } from '../../../core/authz/permissions';
import { APP_PATHS } from '../../../core/router/app-paths';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { EscalationQueueStore } from './escalation-queue.store';

@Component({
  selector: 'app-escalation-queue',
  imports: [DataTableComponent, TuiIcon],
  template: `
    @if (store.items().length === 0 && !store.loading()) {
      <div class="empty">
        <tui-icon icon="@tui.inbox" />
        <p>No escalations in queue</p>
        <span>All clear — no conversations need attention right now.</span>
      </div>
    } @else {
      <app-data-table>
        <table>
          <thead>
            <tr>
              <th scope="col">Reason</th>
              <th scope="col">Required Skills</th>
              <th scope="col">Customer</th>
              <th scope="col">Channel</th>
              <th scope="col">Waiting</th>
              <th scope="col">Actions</th>
            </tr>
          </thead>
          <tbody>
            @for (item of store.items(); track item.escalation.id) {
              <tr>
                <td>{{ item.escalation.reason }}</td>
                <td>
                  @for (skill of item.escalation.requiredSkills; track skill.name) {
                    <span class="skill-chip">{{ skill.name }}</span>
                  }
                </td>
                <td>{{ item.conversation.customer.name }}</td>
                <td class="muted">{{ item.conversation.channel }}</td>
                <td class="muted">{{ formatWaiting(item.waitingSeconds) }}</td>
                <td>
                  @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.escalations])) {
                    <button
                      type="button"
                      class="claim-btn"
                      (click)="store.claim(item.escalation.id)"
                    >
                      Claim
                    </button>
                  }
                </td>
              </tr>
            }
          </tbody>
        </table>
      </app-data-table>
    }
  `,
  styles: [
    `
      :host {
        display: block;
      }
      .skill-chip {
        display: inline-block;
        padding: 2px 8px;
        margin: 1px 4px 1px 0;
        border-radius: 999px;
        background: var(--app-panel-2);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        border: 1px solid var(--app-border);
      }
      .claim-btn {
        padding: 4px 14px;
        border: 1px solid var(--app-accent);
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 600;
        font-size: var(--app-font-sm);
        cursor: pointer;
      }
      .claim-btn:hover {
        background: var(--app-accent-strong);
      }
      .empty {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        padding: 60px 20px;
        color: var(--app-text-3);
        text-align: center;
      }
      .empty p {
        margin: 12px 0 4px;
        font-weight: 600;
        color: var(--app-text-2);
      }
      .empty tui-icon {
        font-size: 48px;
        opacity: 0.4;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class EscalationQueueComponent {
  readonly store = inject(EscalationQueueStore);
  protected readonly permissionsService = inject(PermissionsService);
  protected readonly PAGE_PERMISSIONS = PAGE_PERMISSIONS;
  protected readonly APP_PATHS = APP_PATHS;

  protected formatWaiting(seconds: number): string {
    if (seconds < 60) return '<1m';
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.floor(minutes / 60);
    const remainingMinutes = minutes % 60;
    return `${hours}h ${remainingMinutes}m`;
  }
}
