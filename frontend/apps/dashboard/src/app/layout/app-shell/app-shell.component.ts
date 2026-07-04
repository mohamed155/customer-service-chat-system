import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { Store } from '@ngrx/store';
import { selectSidebarCollapsed } from '../../core/state/app-ui.feature';
import { SidebarComponent } from '../sidebar/sidebar.component';
import { TopbarComponent } from '../topbar/topbar.component';
import { LayoutStore } from './layout.store';

@Component({
  selector: 'app-shell',
  imports: [RouterOutlet, SidebarComponent, TopbarComponent],
  providers: [LayoutStore],
  template: `<div class="shell">
    <app-sidebar [collapsed]="collapsed()" />
    <div class="workspace">
      <app-topbar />
      <main tabindex="-1">
        <div class="content"><router-outlet /></div>
      </main>
    </div>
  </div>`,
  styles: [
    `
      .shell {
        min-height: 100vh;
        display: flex;
        background: var(--app-color-bg);
      }
      .workspace {
        min-width: 0;
        flex: 1;
      }
      main {
        padding: var(--app-space-6);
      }
      .content {
        max-width: var(--app-page-max-width);
        margin: 0 auto;
      }
      main:focus-visible {
        outline: 3px solid var(--app-color-accent);
        outline-offset: -3px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AppShellComponent {
  private readonly store = inject(Store);
  private readonly layoutStore = inject(LayoutStore);
  protected readonly collapsed = this.store.selectSignal(selectSidebarCollapsed);
}
