import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../core/router/app-paths';

@Component({
  selector: 'app-not-found',
  imports: [RouterLink],
  template: `<main>
    <h1>Page not found</h1>
    <p>The page does not exist.</p>
    <a [routerLink]="homeUrl">Return to tenant overview</a>
  </main>`,
  styles: [
    `
      main {
        padding: var(--app-space-8);
        text-align: center;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NotFoundComponent {
  protected readonly homeUrl = `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.overview}`;
}
