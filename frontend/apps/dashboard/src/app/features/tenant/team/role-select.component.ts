import { ChangeDetectionStrategy, Component, computed, inject, input, model } from '@angular/core';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { MembershipRole } from '../../../core/api/tenant-api.models';
import { ChoiceGroupComponent } from '../../../shared/components/choice-group/choice-group.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

const ASSIGNABLE_ROLES: { value: MembershipRole; labelKey: string }[] = [
  { value: 'owner', labelKey: marker('roleNames.owner') },
  { value: 'admin', labelKey: marker('roleNames.admin') },
  { value: 'manager', labelKey: marker('roleNames.manager') },
  { value: 'agent', labelKey: marker('roleNames.agent') },
  { value: 'viewer', labelKey: marker('roleNames.viewer') },
];

const ROLE_RANKS: Record<MembershipRole, number> = {
  owner: 5,
  admin: 4,
  manager: 3,
  agent: 2,
  viewer: 1,
};

@Component({
  selector: 'app-role-select',
  providers: [provideTranslocoScope('team')],
  imports: [ChoiceGroupComponent, TranslocoPipe],
  templateUrl: './role-select.component.html',
  styles: [
    `
      :host {
        display: block;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RoleSelectComponent {
  readonly value = model<MembershipRole>('agent');
  readonly currentRole = input<MembershipRole>('owner');
  readonly canAssignOwner = input(true);
  /** The accessible name of the group; the default is "Role". */
  readonly ariaLabel = input<string | null>(null);

  private readonly locale = inject(LocaleService);

  protected readonly filteredRoles = computed(() => {
    const maxRank = ROLE_RANKS[this.currentRole()] ?? 5;
    return ASSIGNABLE_ROLES.filter((r) => {
      if (r.value === 'owner') {
        return this.currentRole() === 'owner' && this.canAssignOwner();
      }
      return (ROLE_RANKS[r.value] ?? 0) <= maxRank;
    }).map((r) => ({ value: r.value, label: this.locale.translate(r.labelKey) }));
  });

  protected selectRole(role: string): void {
    this.value.set(role as MembershipRole);
  }
}
