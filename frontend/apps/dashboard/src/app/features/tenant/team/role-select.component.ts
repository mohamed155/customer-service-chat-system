import { ChangeDetectionStrategy, Component, computed, input, model } from '@angular/core';
import { MembershipRole } from '../../../core/api/tenant-api.models';
import { ChoiceGroupComponent } from '../../../shared/components/choice-group/choice-group.component';

const ASSIGNABLE_ROLES: { value: MembershipRole; label: string }[] = [
  { value: 'owner', label: 'Owner' },
  { value: 'admin', label: 'Admin' },
  { value: 'manager', label: 'Manager' },
  { value: 'agent', label: 'Support Agent' },
  { value: 'viewer', label: 'Viewer' },
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
  imports: [ChoiceGroupComponent],
  template: `
    <app-choice-group
      [ariaLabel]="ariaLabel()"
      [options]="filteredRoles()"
      [value]="value()"
      (valueChange)="selectRole($event)"
    />
  `,
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
  readonly ariaLabel = input('Role');

  protected readonly filteredRoles = computed(() => {
    const maxRank = ROLE_RANKS[this.currentRole()] ?? 5;
    return ASSIGNABLE_ROLES.filter((r) => {
      if (r.value === 'owner') {
        return this.currentRole() === 'owner' && this.canAssignOwner();
      }
      return (ROLE_RANKS[r.value] ?? 0) <= maxRank;
    });
  });

  protected selectRole(role: string): void {
    this.value.set(role as MembershipRole);
  }
}
