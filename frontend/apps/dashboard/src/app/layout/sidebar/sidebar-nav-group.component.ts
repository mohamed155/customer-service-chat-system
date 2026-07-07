import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'app-sidebar-nav-group',
  host: { '[class.collapsed]': 'collapsed()' },
  template: `
    @if (!collapsed()) {
      <p>{{ label() }}</p>
    }
    <div><ng-content /></div>
  `,
  styles: [
    `
      :host {
        display: grid;
        gap: var(--app-space-2);
      }
      p {
        margin: var(--app-space-3) var(--app-space-2) 0;
        color: var(--app-text-3);
        font-size: 10px;
        font-weight: 700;
        letter-spacing: 0.07em;
        text-transform: uppercase;
      }
      div {
        display: grid;
        gap: 3px;
      }
      :host(.collapsed) {
        justify-items: center;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SidebarNavGroupComponent {
  readonly label = input.required<string>();
  readonly collapsed = input(false);
}
