import { ChangeDetectionStrategy, Component } from '@angular/core';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';

@Component({
  selector: 'app-platform-overview-placeholder',
  imports: [PageContainerComponent, PageHeaderComponent],
  template: `<app-page-container>
    <app-page-header title="Platform overview" />
    <p>Platform workspace foundation.</p>
  </app-page-container>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PlatformOverviewPlaceholderComponent {}
