import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';

@Component({
  selector: 'app-auth-card',
  imports: [TuiIcon],
  template: `
    <main>
      <section>
        <div class="brand">
          <span><tui-icon icon="@tui.sparkles" /></span>
          <strong>Helix</strong>
          <small>Support AI</small>
        </div>
        <h1>{{ title() }}</h1>
        <p>{{ subtitle() }}</p>
        <ng-content />
        <footer><ng-content select="[auth-footer]" /></footer>
      </section>
    </main>
  `,
  styles: [
    `
      main {
        min-height: 100dvh;
        display: grid;
        place-items: center;
        padding: var(--app-space-6);
        background:
          radial-gradient(circle at top, var(--app-accent-soft), transparent 34rem), var(--app-bg);
      }
      section {
        width: min(430px, 100%);
        padding: var(--app-space-8);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-xl);
        background: var(--app-panel);
        box-shadow: var(--app-shadow-lg);
      }
      .brand {
        display: grid;
        justify-items: center;
        gap: 4px;
        margin-bottom: var(--app-space-5);
        text-align: center;
      }
      .brand span {
        width: 38px;
        height: 38px;
        display: grid;
        place-items: center;
        border-radius: var(--app-radius-md);
        background: linear-gradient(135deg, var(--app-accent), var(--app-accent-strong));
        color: var(--app-accent-ink);
      }
      .brand strong {
        color: var(--app-text);
        font-size: var(--app-font-xl);
      }
      .brand small,
      p {
        color: var(--app-text-3);
      }
      h1 {
        margin: 0;
        color: var(--app-text);
        text-align: center;
        font-size: var(--app-font-xl);
      }
      p {
        margin: var(--app-space-2) 0 var(--app-space-5);
        text-align: center;
        font-size: var(--app-font-sm);
      }
      footer {
        margin-top: var(--app-space-5);
        color: var(--app-text-2);
        text-align: center;
        font-size: var(--app-font-sm);
      }
      footer:empty {
        display: none;
      }
      :host ::ng-deep form {
        display: grid;
        gap: var(--app-space-4);
      }
      :host ::ng-deep label {
        display: grid;
        gap: 7px;
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        font-weight: 650;
      }
      :host ::ng-deep input {
        height: 40px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
        font: inherit;
      }
      :host ::ng-deep input:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        border-color: var(--app-accent);
      }
      :host ::ng-deep button[type='submit'] {
        height: 40px;
        border: 1px solid var(--app-accent);
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 700;
        cursor: pointer;
      }
      :host ::ng-deep a {
        color: var(--app-accent-strong);
        font-weight: 650;
        text-decoration: none;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AuthCardComponent {
  readonly title = input.required<string>();
  readonly subtitle = input.required<string>();
}
