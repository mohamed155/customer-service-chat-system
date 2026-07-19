import { Component, Input, ChangeDetectionStrategy } from '@angular/core';

@Component({
  selector: 'hx-handoff-banner',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="handoff">
      @if (teamOnline) {
        <span>Connecting you to a support agent…</span>
      } @else {
        <span>Our team is away — we'll reply as soon as someone is back.</span>
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
  @Input({ required: true }) teamOnline = false;
}
