import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../../core/router/app-paths';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';

@Component({
  selector: 'app-tenant-select',
  imports: [EmptyStateComponent, PageContainerComponent, PageHeaderComponent, RouterLink],
  template: `
    <app-page-container>
      <app-page-header
        title="Select a workspace"
        description="Choose a workspace from the tenant switcher in the topbar to get started."
      />
      @if (hasMemberships()) {
        <app-empty-state
          icon="@tui.ban"
          title="Select a tenant to get started"
          description="You are a member of one or more workspaces. Choose a tenant from the tenant switcher in the topbar to access your conversations, customers, and settings. If you don't see a tenant switcher, contact your workspace administrator to ensure you have the correct access permissions."
        >
          <a [routerLink]="APP_PATHS.tenant.base">Back to tenant area</a>
        </app-empty-state>
      } @else {
        <app-empty-state
          icon="@tui.shield-off"
          title="No workspace access"
          description="You don't currently have access to any workspace. Contact your workspace administrator to get the correct permissions. If you believe this is an error, try signing out and back in."
        />
      }
    </app-page-container>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TenantSelectComponent {
  private readonly currentUser = inject(CurrentUserService);
  protected readonly APP_PATHS = APP_PATHS;
  protected readonly hasMemberships = computed(() => {
    const user = this.currentUser.currentUser();
    return user?.memberships != null && user.memberships.length > 0;
  });
}
