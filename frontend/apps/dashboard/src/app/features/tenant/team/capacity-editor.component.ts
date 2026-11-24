import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { TranslocoPipe } from '@jsverse/transloco';
import { TeamMember } from '../../../core/api/tenant-api.models';
import { AgentCapacity } from '../../../core/api/routing-capacity-api.service';
import { ApiError } from '../../../core/api/api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { TeamStore } from './team.store';

@Component({
  selector: 'app-capacity-editor',
  imports: [TranslocoPipe, ButtonComponent, InlineAlertComponent, ErrorReferenceComponent],
  templateUrl: './capacity-editor.component.html',
  styles: [
    `
      .capacity-editor {
        display: grid;
        gap: var(--app-space-1);
        min-width: 160px;
      }
      .capacity-controls {
        display: flex;
        align-items: center;
        gap: var(--app-space-1);
      }
      .capacity-controls input {
        width: 76px;
        min-height: 36px;
        padding-inline: var(--app-space-2);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
      }
      .capacity-summary {
        color: var(--app-text-3);
        font-size: 12px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CapacityEditorComponent {
  readonly member = input.required<TeamMember>();
  readonly canManage = input.required<boolean>();
  protected readonly store = inject(TeamStore);
  private readonly locale = inject(LocaleService);
  protected readonly capacity = computed(() => {
    const agents = this.store.routingCapacity()?.agents;
    return Array.isArray(agents)
      ? agents.find((agent) => agent?.membership_id === this.member().id)
      : undefined;
  });

  protected capacityOverrideLabel(capacity: AgentCapacity): string {
    const override = capacity['override'];
    return override === null || override === undefined
      ? this.locale.translate(marker('team.capacity.inherit'))
      : String(override);
  }

  protected updateAgentCapacity(membershipId: string, rawCapacity: string): void {
    const capacity = rawCapacity.trim() === '' ? null : Number(rawCapacity);
    if (capacity !== null && (!Number.isInteger(capacity) || capacity < 0 || capacity > 200))
      return;
    this.store.setMemberCapacity({ membershipId, capacity });
  }

  protected errorKey(error: ApiError | null): string {
    return errorMessageKey(error ?? { code: 'unknown_error', message: '', status: 0 });
  }
}
