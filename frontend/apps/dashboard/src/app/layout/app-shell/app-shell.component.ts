import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { Store } from '@ngrx/store';
import { TuiIcon } from '@taiga-ui/core';
import { ApiErrorNotificationService } from '../../core/errors/api-error-notification.service';
import { selectSidebarCollapsed } from '../../core/state/app-ui.feature';
import { SidebarComponent } from '../sidebar/sidebar.component';
import { TopbarComponent } from '../topbar/topbar.component';
import { LayoutStore } from './layout.store';

@Component({
  selector: 'app-shell',
  imports: [RouterOutlet, SidebarComponent, TopbarComponent, TuiIcon],
  providers: [LayoutStore],
  template: `<div class="shell">
    <app-sidebar [collapsed]="collapsed()" />
    <div class="workspace">
      <app-topbar />
      @if (errorMessage(); as message) {
        <section class="tenant-error-banner" role="alert" aria-live="polite">
          <tui-icon icon="@tui.triangle-alert" />
          <span>{{ message }}</span>
          <button type="button" (click)="dismissError()" aria-label="Dismiss tenant access alert">
            Dismiss
          </button>
        </section>
      }
      <main tabindex="-1"><router-outlet /></main>
    </div>
  </div>`,
  styles: [
    `
      .shell {
        height: 100dvh;
        display: grid;
        grid-template-columns: auto 1fr;
        overflow: hidden;
        background: var(--app-bg);
      }
      .workspace {
        min-width: 0;
        display: flex;
        flex-direction: column;
        overflow: hidden;
      }
      .tenant-error-banner {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        margin: var(--app-space-3) var(--app-page-padding-x) 0;
        padding: var(--app-space-3);
        border: 1px solid var(--app-danger, #d92d20);
        border-radius: var(--app-radius-md);
        background: color-mix(in srgb, var(--app-danger, #d92d20) 10%, var(--app-panel));
        color: var(--app-text);
        font-size: var(--app-font-sm);
      }
      .tenant-error-banner tui-icon {
        color: var(--app-danger, #d92d20);
        flex: 0 0 auto;
      }
      .tenant-error-banner span {
        flex: 1;
      }
      .tenant-error-banner button {
        border: 0;
        background: transparent;
        color: var(--app-accent);
        cursor: pointer;
        font: inherit;
        font-weight: 700;
      }
      .tenant-error-banner button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      main {
        min-height: 0;
        flex: 1;
        overflow-y: auto;
      }
      main:focus-visible {
        outline: 3px solid var(--app-accent);
        outline-offset: -3px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AppShellComponent {
  private readonly store = inject(Store);
  private readonly layoutStore = inject(LayoutStore);
  private readonly errorNotifications = inject(ApiErrorNotificationService);
  protected readonly collapsed = this.store.selectSignal(selectSidebarCollapsed);
  protected readonly errorMessage = this.errorNotifications.message;

  protected dismissError(): void {
    this.errorNotifications.clear();
  }
}
