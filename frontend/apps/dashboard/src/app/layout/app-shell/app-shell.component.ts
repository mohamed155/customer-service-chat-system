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
  protected readonly collapsed = this.store.selectSignal(selectSidebarCollapsed);
}
