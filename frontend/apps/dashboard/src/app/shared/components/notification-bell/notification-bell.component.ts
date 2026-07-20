import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';

@Component({
  selector: 'app-notification-bell',
  imports: [TuiIcon],
  template: `
    <button
      type="button"
      class="bell-button"
      aria-label="Notifications"
      (click)="togglePanel.emit()"
    >
      <tui-icon icon="@tui.bell" />
      @if (count() > 0) {
        <span class="badge">{{ count() > 99 ? '99+' : count() }}</span>
      }
    </button>
  `,
  styles: [
    `
      .bell-button {
        position: relative;
        width: 38px;
        height: 38px;
        display: inline-grid;
        place-items: center;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text-2);
        cursor: pointer;
        transition:
          background var(--app-transition-fast),
          border-color var(--app-transition-fast),
          color var(--app-transition-fast);
      }
      .bell-button:hover {
        background: var(--app-panel-2);
        border-color: var(--app-border-strong);
        color: var(--app-text);
      }
      .bell-button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-color: var(--app-accent);
      }
      .bell-button tui-icon {
        font-size: 17px;
      }
      .badge {
        position: absolute;
        top: -4px;
        right: -4px;
        min-width: 16px;
        height: 16px;
        padding: 0 4px;
        border-radius: 999px;
        background: var(--tui-status-danger, #dc2626);
        color: #fff;
        font-size: 10px;
        font-weight: 700;
        line-height: 16px;
        text-align: center;
        pointer-events: none;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NotificationBellComponent {
  readonly count = input(0);
  readonly togglePanel = output<void>();
}
