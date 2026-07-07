import { ChangeDetectionStrategy, Component } from '@angular/core';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';

@Component({
  selector: 'app-knowledge-base',
  imports: [PageContainerComponent],
  template: `<app-page-container><p>Knowledge Base — coming soon</p></app-page-container>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class KnowledgeBaseComponent {}
