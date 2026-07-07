import { ChangeDetectionStrategy, Component } from '@angular/core';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';

@Component({
  selector: 'app-settings',
  imports: [PageContainerComponent],
  template: `<app-page-container><p>Settings — coming soon</p></app-page-container>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SettingsComponent {}
