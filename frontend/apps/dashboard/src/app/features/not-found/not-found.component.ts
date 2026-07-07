import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../core/router/app-paths';

@Component({
  selector: 'app-not-found',
  imports: [RouterLink],
  template: `<main>
    <section>
      <span>404</span>
      <h1>Page not found</h1>
      <p>The route you opened does not map to a Helix workspace screen.</p>
      <a [routerLink]="homeUrl">Return to overview</a>
    </section>
  </main>`,
  styles: [
    `
      main {
        min-height: 100dvh;
        display: grid;
        place-items: center;
        padding: var(--app-space-8);
        background: var(--app-bg);
      }
      section {
        width: min(420px, 100%);
        padding: var(--app-space-8);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-xl);
        background: var(--app-panel);
        box-shadow: var(--app-shadow-lg);
        text-align: center;
      }
      span {
        display: inline-flex;
        margin-bottom: var(--app-space-3);
        padding: 4px 10px;
        border-radius: 999px;
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
        font-size: var(--app-font-xs);
        font-weight: 700;
      }
      h1 {
        margin: 0;
        color: var(--app-text);
        font-size: var(--app-font-xl);
      }
      p {
        margin: var(--app-space-3) 0 var(--app-space-5);
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
      }
      a {
        display: inline-flex;
        align-items: center;
        height: 38px;
        padding: 0 var(--app-space-4);
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 700;
        text-decoration: none;
      }
      a:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NotFoundComponent {
  protected readonly homeUrl = `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.overview}`;
}
