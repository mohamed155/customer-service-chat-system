import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../../core/router/app-paths';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { TranslocoPipe } from '@jsverse/transloco';

@Component({
  selector: 'app-tenant-select',
  imports: [
    EmptyStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    RouterLink,
    TranslocoPipe,
  ],
  templateUrl: './tenant-select.component.html',
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
