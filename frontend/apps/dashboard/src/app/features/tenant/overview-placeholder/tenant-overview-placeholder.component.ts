import { ChangeDetectionStrategy, Component } from '@angular/core';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';

@Component({
  selector: 'app-tenant-overview-placeholder',
  imports: [PageHeaderComponent],
  template: `<app-page-header title="Tenant overview" />
    <p>Tenant workspace foundation.</p>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TenantOverviewPlaceholderComponent {}
