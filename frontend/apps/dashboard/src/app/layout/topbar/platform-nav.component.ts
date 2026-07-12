import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  afterNextRender,
  computed,
  inject,
  signal,
} from '@angular/core';
import { Router } from '@angular/router';
import { TuiIcon } from '@taiga-ui/core';
import { Permission } from '../../core/authz/permissions';
import { PermissionsService } from '../../core/authz/permissions.service';
import { APP_PATHS } from '../../core/router/app-paths';

interface PlatformDestination {
  readonly label: string;
  readonly path: string;
  readonly permission: Permission;
}

const PLATFORM_DESTINATIONS: readonly PlatformDestination[] = [
  {
    label: 'Tenants',
    path: `/${APP_PATHS.platform.base}/${APP_PATHS.platform.tenants}`,
    permission: 'platform.tenants.list',
  },
  {
    label: 'Platform overview',
    path: `/${APP_PATHS.platform.base}`,
    permission: 'platform.admin',
  },
] as const;

@Component({
  selector: 'app-platform-nav',
  imports: [TuiIcon],
  template: `
    @if (destinations().length) {
      <div class="nav">
        <button
          type="button"
          class="trigger"
          (click)="toggle()"
          [attr.aria-expanded]="open()"
          aria-haspopup="menu"
          aria-label="Platform"
        >
          <tui-icon icon="@tui.layout-dashboard" />
          <span>Platform</span>
        </button>

        @if (open()) {
          <div
            class="dropdown"
            role="menu"
            tabindex="-1"
            (click)="close()"
            (keydown)="onDropdownKeydown($event)"
          >
            @for (dest of destinations(); track dest.path) {
              <button
                type="button"
                class="option"
                role="menuitem"
                (click)="navigate(dest)"
                (keydown.enter)="navigate(dest)"
              >
                {{ dest.label }}
              </button>
            }
          </div>
        }
      </div>
    }
  `,
  styles: [
    `
      .nav {
        position: relative;
      }
      .trigger {
        height: 38px;
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-2);
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        cursor: pointer;
        font: inherit;
        white-space: nowrap;
      }
      .trigger:hover {
        background: var(--app-panel-2);
        border-color: var(--app-border-strong);
      }
      .trigger:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      .dropdown {
        position: absolute;
        top: calc(100% + 4px);
        right: 0;
        min-width: 200px;
        background: var(--app-panel);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        box-shadow: var(--app-shadow-lg);
        z-index: 100;
        overflow: hidden;
      }
      .option {
        width: 100%;
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        padding: var(--app-space-2) var(--app-space-3);
        border: 0;
        background: transparent;
        color: var(--app-text);
        cursor: pointer;
        text-align: left;
        font: inherit;
        white-space: nowrap;
      }
      .option:hover {
        background: var(--app-fill-hover);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PlatformNavComponent {
  private readonly permissions = inject(PermissionsService);
  private readonly router = inject(Router);
  private readonly elementRef = inject(ElementRef);

  protected readonly open = signal(false);
  protected readonly destinations = computed(() =>
    PLATFORM_DESTINATIONS.filter((d) => this.permissions.has(d.permission)),
  );

  constructor() {
    afterNextRender(() => {
      if (this.open()) {
        this.focusFirstOption();
      }
    });
  }

  toggle(): void {
    this.open.update((v) => !v);
    if (!this.open()) {
      this.focusTrigger();
    }
  }

  close(): void {
    this.open.set(false);
    this.focusTrigger();
  }

  navigate(dest: PlatformDestination): void {
    this.close();
    this.router.navigate([dest.path]);
  }

  protected onDropdownKeydown(event: KeyboardEvent): void {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const items = Array.from(
        this.elementRef.nativeElement.querySelectorAll('.option'),
      ) as HTMLElement[];
      const current = document.activeElement as HTMLElement | null;
      const currentIndex = items.indexOf(current as HTMLElement);
      let nextIndex: number;
      if (event.key === 'ArrowDown') {
        nextIndex = currentIndex < items.length - 1 ? currentIndex + 1 : 0;
      } else {
        nextIndex = currentIndex > 0 ? currentIndex - 1 : items.length - 1;
      }
      items[nextIndex]?.focus();
    }
  }

  private focusTrigger(): void {
    (this.elementRef.nativeElement.querySelector('.trigger') as HTMLElement)?.focus();
  }

  private focusFirstOption(): void {
    const items = Array.from(
      this.elementRef.nativeElement.querySelectorAll('.option'),
    ) as HTMLElement[];
    items[0]?.focus();
  }

  @HostListener('document:click', ['$event'])
  onDocumentClick(event: MouseEvent): void {
    if (!this.open()) return;
    if (!this.elementRef.nativeElement.contains(event.target)) {
      this.close();
    }
  }

  @HostListener('document:keydown.escape')
  onEscape(): void {
    this.close();
  }
}
