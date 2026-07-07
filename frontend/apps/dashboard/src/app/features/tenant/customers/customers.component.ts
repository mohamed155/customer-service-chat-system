import { ChangeDetectionStrategy, Component } from '@angular/core';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';

@Component({
  selector: 'app-customers',
  imports: [PageContainerComponent],
  template: `<app-page-container><p>Customers — coming soon</p></app-page-container>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CustomersComponent {}
