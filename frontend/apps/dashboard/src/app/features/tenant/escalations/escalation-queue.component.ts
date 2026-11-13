import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { EscalationQueueStore } from './escalation-queue.store';
import { ApiError } from '../../../core/api/api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { marker } from '../../../core/i18n/marker';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

@Component({
  selector: 'app-escalation-queue',
  imports: [DataTableComponent, ErrorReferenceComponent, TuiIcon, TranslocoPipe],
  templateUrl: './escalation-queue.component.html',
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
      .error-banner {
        padding: 8px 16px;
        margin-bottom: 12px;
        border-radius: var(--app-radius-md);
        background: var(--app-danger-bg, #fee2e2);
        border: 1px solid var(--app-danger, #dc2626);
        color: var(--app-danger, #dc2626);
        font-size: var(--app-font-sm);
        font-weight: 600;
      }
    `,
  ],
  providers: [provideTranslocoScope('conversations'), EscalationQueueStore],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class EscalationQueueComponent {
  readonly store = inject(EscalationQueueStore);
  protected readonly permissionsService = inject(PermissionsService);

  /** Catalog key of the text for an error of this page; a failed claim is a lost race. */
  protected errorKey(error: ApiError): string {
    return errorMessageKey(error, { 409: marker('conversations.escalations.alreadyClaimed') });
  }

  /** The catalog key of how long an escalation has waited, and the values it needs. */
  protected waitingText(seconds: number): { key: string; params?: Record<string, number> } {
    if (seconds < 60) return { key: marker('conversations.escalations.waitLessThanMinute') };
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60)
      return { key: marker('conversations.escalations.waitMinutes'), params: { minutes } };
    return {
      key: marker('conversations.escalations.waitHours'),
      params: { hours: Math.floor(minutes / 60), minutes: minutes % 60 },
    };
  }
}
