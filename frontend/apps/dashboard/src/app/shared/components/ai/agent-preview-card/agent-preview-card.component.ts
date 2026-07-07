import { ChangeDetectionStrategy, Component } from '@angular/core';
import { AvatarComponent } from '../../avatar/avatar.component';

@Component({
  selector: 'app-agent-preview-card',
  imports: [AvatarComponent],
  template: `
    <div class="row customer">
      <app-avatar initials="MC" size="sm" />
      <p>Can I exchange an order without losing the promotion?</p>
    </div>
    <div class="row ai">
      <app-avatar initials="AI" size="sm" />
      <p>Yes. I can preserve eligible promotional credit while starting the exchange.</p>
    </div>
  `,
  styles: [
    `
      :host {
        display: grid;
        gap: var(--app-space-3);
      }
      .row {
        display: flex;
        align-items: flex-start;
        gap: var(--app-space-2);
      }
      p {
        margin: 0;
        padding: var(--app-space-3);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel-2);
        color: var(--app-text);
        font-size: var(--app-font-sm);
        line-height: 1.45;
      }
      .ai p {
        background: var(--app-accent-soft);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AgentPreviewCardComponent {}
