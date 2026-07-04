import { ChangeDetectionStrategy, Component } from '@angular/core';

@Component({
  selector: 'app-login-placeholder',
  template: `<main class="auth-page">
    <section>
      <h1>Sign in</h1>
      <p>Authentication is coming next.</p>
    </section>
  </main>`,
  styles: [
    `
      .auth-page {
        min-height: 100vh;
        display: grid;
        place-items: center;
        padding: var(--app-space-6);
      }
      section {
        padding: var(--app-space-8);
        border-radius: var(--app-radius-lg);
        background: var(--app-color-surface);
        box-shadow: var(--app-shadow-md);
        text-align: center;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LoginPlaceholderComponent {}
