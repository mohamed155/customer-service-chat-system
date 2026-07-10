import { ChangeDetectionStrategy, Component, inject, HostListener } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { Store } from '@ngrx/store';
import { TuiIcon } from '@taiga-ui/core';
import { ApiErrorNotificationService } from '../../core/errors/api-error-notification.service';
import { selectSidebarCollapsed } from '../../core/state/app-ui.feature';
import { BreadcrumbComponent } from '../breadcrumb/breadcrumb.component';
import { SidebarComponent } from '../sidebar/sidebar.component';
import { TopbarComponent } from '../topbar/topbar.component';
import { LayoutStore } from './layout.store';

@Component({
  selector: 'app-shell',
  imports: [BreadcrumbComponent, RouterOutlet, SidebarComponent, TopbarComponent, TuiIcon],
  providers: [LayoutStore],
  template: `<div class="shell" (keydown.escape)="closeDrawer()">
    <div
      class="sidebar-wrapper"
      [class.drawer]="isMobile()"
      [class.open]="isMobile() && drawerOpen()"
    >
      <app-sidebar [collapsed]="collapsed()" />
    </div>
    @if (isMobile() && drawerOpen()) {
      <div class="scrim" (click)="closeDrawer()"></div>
    }
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
      <main tabindex="-1"><app-breadcrumb /><router-outlet /></main>
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
      .sidebar-wrapper {
        display: contents;
      }
      .sidebar-wrapper.drawer {
        display: block;
        position: fixed;
        top: 0;
        left: 0;
        bottom: 0;
        z-index: 200;
        transform: translateX(-100%);
        transition: transform var(--app-transition-base);
      }
      .sidebar-wrapper.drawer.open {
        transform: translateX(0);
      }
      .scrim {
        position: fixed;
        inset: 0;
        z-index: 199;
        background: rgba(0, 0, 0, 0.4);
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
  protected readonly isMobile = this.layoutStore.isMobile;
  protected readonly drawerOpen = this.layoutStore.drawerOpen;

  protected dismissError(): void {
    this.errorNotifications.clear();
  }

  protected closeDrawer(): void {
    this.layoutStore.closeDrawer();
  }
}
