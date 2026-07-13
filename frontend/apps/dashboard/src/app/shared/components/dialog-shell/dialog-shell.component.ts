import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  ViewChild,
  effect,
  inject,
  input,
  output,
} from '@angular/core';

@Component({
  selector: 'app-dialog-shell',
  template: `
    @if (open()) {
      <button
        class="dialog-backdrop"
        type="button"
        aria-label="Close dialog"
        [disabled]="dismissDisabled()"
        (click)="requestDismiss()"
      ></button>
      <section
        #panel
        class="dialog"
        [attr.role]="role()"
        aria-modal="true"
        [attr.aria-labelledby]="ariaLabelledby()"
        [attr.aria-describedby]="ariaDescribedby()"
        tabindex="-1"
        (keydown)="onKeydown($event)"
      >
        <ng-content />
      </section>
    }
  `,
  styles: [
    `
      :host {
        display: contents;
      }
      .dialog-backdrop {
        position: fixed;
        inset: 0;
        z-index: 99;
        border: 0;
        background: rgba(0, 0, 0, 0.4);
      }
      .dialog-backdrop:disabled {
        cursor: default;
      }
      .dialog {
        position: fixed;
        top: 50%;
        left: 50%;
        z-index: 100;
        width: min(440px, calc(100vw - 2rem));
        max-height: min(90dvh, 52rem);
        overflow: auto;
        transform: translate(-50%, -50%);
        padding: var(--app-space-5);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
        box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
      }
      .dialog:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DialogShellComponent {
  readonly open = input(false);
  readonly role = input<'dialog' | 'alertdialog'>('dialog');
  readonly ariaLabelledby = input<string | null>(null);
  readonly ariaDescribedby = input<string | null>(null);
  readonly dismissDisabled = input(false);

  readonly dismiss = output<void>();

  @ViewChild('panel') private readonly panel?: ElementRef<HTMLElement>;

  private readonly destroyRef = inject(DestroyRef);
  private previouslyFocused: HTMLElement | null = null;
  private wasOpen = false;

  constructor() {
    effect(() => {
      const isOpen = this.open();
      if (isOpen && !this.wasOpen) {
        this.previouslyFocused = document.activeElement as HTMLElement | null;
        queueMicrotask(() => this.focusInitial());
      }
      if (!isOpen && this.wasOpen) {
        queueMicrotask(() => this.restoreFocus());
      }
      this.wasOpen = isOpen;
    });

    this.destroyRef.onDestroy(() => this.restoreFocus());
  }

  protected requestDismiss(): void {
    if (this.dismissDisabled()) return;
    this.dismiss.emit();
  }

  protected onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      this.requestDismiss();
      return;
    }

    if (event.key !== 'Tab') return;

    const panel = this.panel?.nativeElement;
    if (!panel) return;

    const focusable = Array.from(
      panel.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    ).filter((element) => !element.hasAttribute('disabled'));

    if (focusable.length === 0) return;

    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement as HTMLElement | null;

    if (event.shiftKey && active === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }

  private focusInitial(): void {
    if (!this.open()) return;

    const panel = this.panel?.nativeElement;
    if (!panel) return;

    const focusable = panel.querySelector<HTMLElement>(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    );

    if (focusable) {
      focusable.focus();
      return;
    }

    panel.focus();
  }

  private restoreFocus(): void {
    this.previouslyFocused?.focus();
    this.previouslyFocused = null;
  }
}
