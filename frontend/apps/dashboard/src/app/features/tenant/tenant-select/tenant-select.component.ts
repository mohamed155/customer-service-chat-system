import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterLink } from '@angular/router';
import { TuiIcon } from '@taiga-ui/core';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { APP_PATHS } from '../../../core/router/app-paths';

@Component({
  selector: 'app-tenant-select',
  imports: [PageContainerComponent, RouterLink, TuiIcon],
  template: `
    <app-page-container>
      <div class="select-tenant">
        <tui-icon
          class="select-tenant-icon"
          icon="@tui.ban"
          [style.width.px]="48"
          [style.height.px]="48"
        />
        <h2>Select a tenant to get started</h2>
        <p>
          You are a member of one or more workspaces. Choose a tenant from the
          tenant switcher in the sidebar to access your conversations, customers,
          and settings.
        </p>
        <p>
          If you don't see a tenant switcher, contact your workspace administrator
          to ensure you have the correct access permissions.
        </p>
        <a [routerLink]="APP_PATHS.tenant.base" tuiLink>Back to tenant area</a>
      </div>
    </app-page-container>
  `,
  styles: [
    `
      .select-tenant {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        text-align: center;
        gap: var(--app-space-4);
        padding: var(--app-space-12) var(--app-space-4);
        max-width: 480px;
        margin: 0 auto;
      }
      .select-tenant-icon {
        color: var(--app-text-3);
      }
      h2 {
        margin: 0;
        font-size: var(--app-font-xl);
        font-weight: 600;
        color: var(--app-text);
      }
      p {
        margin: 0;
        color: var(--app-text-2);
        font-size: var(--app-font-md);
        line-height: 1.6;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TenantSelectComponent {
  protected readonly APP_PATHS = APP_PATHS;
}