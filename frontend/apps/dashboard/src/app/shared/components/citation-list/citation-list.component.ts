import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { RouterLink } from '@angular/router';
import { TuiIcon } from '@taiga-ui/core';
import { APP_PATHS } from '../../../core/router/app-paths';
import { Citation } from '../../../core/api/tenant-api.models';

@Component({
  selector: 'app-citation-list',
  imports: [RouterLink, TuiIcon],
  templateUrl: './citation-list.component.html',
  styleUrl: './citation-list.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CitationListComponent {
  readonly citations = input.required<readonly Citation[]>();
  protected readonly appPaths = APP_PATHS;
}
