import { Component, OnInit, inject, DestroyRef } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { WidgetApiService } from './core/widget-api.service';
import { SessionStore } from './core/session.store';
import { WidgetStore } from './core/widget.store';
import { ChatWindowComponent } from './components/chat-window.component';

@Component({
  selector: 'hx-widget-root',
  standalone: true,
  imports: [ChatWindowComponent],
  template: `
    @if (store.uiState() !== 'closed') {
      <hx-chat-window (closeWindow)="onClose()" (resizeWindow)="onResize($any($event))" />
    }
  `,
  styles: [
    `
      :host {
        display: block;
        width: 100%;
        height: 100%;
        font-family: var(--wgt-font);
      }
    `,
  ],
})
export class AppComponent implements OnInit {
  private readonly api = inject(WidgetApiService);
  private readonly session = inject(SessionStore);
  readonly store = inject(WidgetStore);
  private readonly destroyRef = inject(DestroyRef);

  ngOnInit(): void {
    const params = new URLSearchParams(window.location.search);
    const widgetId = params.get('id');
    if (!widgetId) return;

    this.api
      .getConfig(widgetId)
      .pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe({
        next: (config) => {
          this.store.setConfig(config);
          document.documentElement.style.setProperty('--wgt-primary', config.primaryColor);
          document.documentElement.setAttribute('data-wgt-theme', config.theme);

          this.store.initSession(widgetId);

          const token = this.session.getToken();
          if (token) {
            this.store.open();
          }

          this.emitResize();
        },
      });
  }

  onClose(): void {
    window.parent.postMessage({ source: 'hx-widget', type: 'close', width: 0, height: 0 }, '*');
  }

  onResize(dim: { width: number; height: number }): void {
    window.parent.postMessage({ source: 'hx-widget', type: 'resize', ...dim }, '*');
  }

  private emitResize(): void {
    requestAnimationFrame(() => {
      const h = document.documentElement.scrollHeight;
      const w = document.documentElement.scrollWidth;
      window.parent.postMessage({ source: 'hx-widget', type: 'resize', width: w, height: h }, '*');
    });
  }
}
