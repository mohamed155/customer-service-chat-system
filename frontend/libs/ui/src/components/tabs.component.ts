import { Component, EventEmitter, Input, Output } from '@angular/core';
@Component({
  selector: 'hx-tabs',
  standalone: true,
  template: `<div class="hx-tabs" role="tablist" [attr.aria-label]="label">
    @for (tab of tabs; track tab; let i = $index) {
      <button
        class="hx-tabs__tab"
        [class.hx-tabs__tab--active]="i === active"
        role="tab"
        [attr.aria-selected]="i === active"
        (click)="select(i)"
        (keydown.enter)="select(i)"
        (keydown.space)="select(i); $event.preventDefault()"
      >
        {{ tab }}
      </button>
    }
  </div>`,
  styles: [
    `
      .hx-tabs {
        border-bottom: 1px solid var(--border);
        display: flex;
      }
      .hx-tabs__tab {
        background: var(--panel);
        border: 1px solid var(--border);
        color: var(--text-2);
        padding: 8px;
      }
      .hx-tabs__tab--active {
        background: var(--accent-soft);
        color: var(--accent-strong);
      }
    `,
  ],
})
export class TabsComponent {
  @Input() tabs: string[] = [];
  @Input() active = 0;
  @Input() label = 'Tabs';
  @Output() activeChange = new EventEmitter<number>();
  select(i: number) {
    this.active = i;
    this.activeChange.emit(i);
  }
}
