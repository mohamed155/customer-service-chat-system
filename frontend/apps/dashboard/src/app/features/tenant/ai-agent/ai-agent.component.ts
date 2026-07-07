import { ChangeDetectionStrategy, Component } from '@angular/core';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';

@Component({
  selector: 'app-ai-agent',
  imports: [PageContainerComponent],
  template: `<app-page-container><p>AI Agent — coming soon</p></app-page-container>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AiAgentComponent {}
