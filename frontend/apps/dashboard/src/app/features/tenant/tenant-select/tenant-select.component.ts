import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../../core/router/app-paths';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';

@Component({
  selector: 'app-tenant-select',
  imports: [EmptyStateComponent, PageContainerComponent, RouterLink],
  template: `
    <app-page-container>
      <app-empty-state
        icon="@tui.ban"
        title="Select a tenant to get started"
        description="You are a member of one or more workspaces. Choose a tenant from the tenant switcher in the topbar to access your conversations, customers, and settings. If you don't see a tenant switcher, contact your workspace administrator to ensure you have the correct access permissions."
      >
        <a [routerLink]="APP_PATHS.tenant.base">Back to tenant area</a>
      </app-empty-state>
    </app-page-container>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TenantSelectComponent {
  protected readonly APP_PATHS = APP_PATHS;
}
