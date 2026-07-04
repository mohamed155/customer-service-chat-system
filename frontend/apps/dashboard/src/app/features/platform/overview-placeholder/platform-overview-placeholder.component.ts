import { ChangeDetectionStrategy, Component } from '@angular/core';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';

@Component({
  selector: 'app-platform-overview-placeholder',
  imports: [PageHeaderComponent],
  template: `<app-page-header title="Platform overview" />
    <p>Platform workspace foundation.</p>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PlatformOverviewPlaceholderComponent {}
