import { Component, Input, ChangeDetectionStrategy, inject } from '@angular/core';

import { I18n } from '../i18n/i18n.service';

@Component({
  selector: 'hx-handoff-banner',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="handoff">
      @if (teamOnline) {
        <span>{{ i18n.t('handoff.connecting') }}</span>
      } @else {
        <span>{{ i18n.t('handoff.away') }}</span>
      }
    </div>
  `,
  styles: [
    `
      .handoff {
        padding: 10px 14px;
        text-align: center;
        font-size: 13px;
        color: var(--wgt-muted-text);
        background: var(--wgt-surface);
        border-top: 1px solid var(--wgt-border);
      }
    `,
  ],
})
export class HandoffBannerComponent {
  protected readonly i18n = inject(I18n);
  @Input({ required: true }) teamOnline = false;
}
