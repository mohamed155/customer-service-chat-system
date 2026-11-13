import {
  ChangeDetectionStrategy,
  Component,
  computed,
  ElementRef,
  inject,
  QueryList,
  signal,
  ViewChild,
  ViewChildren,
} from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { AuthService } from '../../core/auth/auth.service';
import { roleLine } from '../../core/identity/role-display';
import { CurrentUserService } from '../../core/tenant/current-user.service';
import { TenantContextService } from '../../core/tenant/tenant-context.service';
import { AvatarComponent } from '../../shared/components/avatar/avatar.component';
import { TranslocoPipe } from '@jsverse/transloco';

@Component({
  selector: 'app-user-menu',
  imports: [AvatarComponent, TuiIcon, TranslocoPipe],
  templateUrl: './user-menu.component.html',
  styles: [
    `
      .menu {
        position: relative;
      }
      .trigger {
        height: 38px;
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-1);
        padding: 0 var(--app-space-2);
        border: 1px solid transparent;
        border-radius: var(--app-radius-md);
        background: transparent;
        color: var(--app-text);
        cursor: pointer;
        font: inherit;
      }
      .trigger:hover {
        background: var(--app-fill-hover);
        border-color: var(--app-border);
      }
      .trigger:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      .dropdown {
        position: absolute;
        top: calc(100% + 4px);
        right: 0;
        width: 240px;
        background: var(--app-panel);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        box-shadow: var(--app-shadow-lg);
        z-index: 100;
        overflow: hidden;
      }
      .user-info {
        display: flex;
        flex-direction: column;
        gap: var(--app-space-1);
        padding: var(--app-space-3);
      }
      .display-name {
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 600;
      }
      .email {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .role-line {
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        margin-top: var(--app-space-1);
      }
      .divider {
        height: 1px;
        background: var(--app-border);
        margin: 0;
      }
      .sign-out {
        width: 100%;
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-2);
        padding: var(--app-space-2) var(--app-space-3);
        border: 0;
        background: transparent;
        color: var(--app-text);
        cursor: pointer;
        text-align: left;
        font: inherit;
        font-size: var(--app-font-sm);
      }
      .sign-out:hover {
        background: var(--app-fill-hover);
      }
      .sign-out:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: -3px;
      }
    `,
  ],
  host: {
    '(document:click)': 'handleClick($event)',
    '(keydown.escape)': 'close()',
    '(keydown.arrowdown)': 'handleArrowDown($event)',
    '(keydown.arrowup)': 'handleArrowUp($event)',
  },
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UserMenuComponent {
  private readonly auth = inject(AuthService);
  private readonly currentUserService = inject(CurrentUserService);
  private readonly tenantContext = inject(TenantContextService);
  private readonly elementRef = inject(ElementRef);

  protected readonly currentUser = this.currentUserService.currentUser;
  protected readonly open = signal(false);

  @ViewChild('trigger', { read: ElementRef })
  protected triggerButton?: ElementRef<HTMLElement>;

  @ViewChildren('menuItem')
  protected menuItems!: QueryList<ElementRef<HTMLElement>>;

  protected readonly initials = computed(() => {
    const name = this.currentUser()?.displayName;
    return name
      ? name
          .split(' ')
          .map((w) => w[0])
          .join('')
          .toUpperCase()
          .slice(0, 2)
      : '?';
  });

  protected readonly roleLine = computed(() =>
    roleLine(this.currentUser(), this.tenantContext.activeTenant()),
  );

  toggle(): void {
    this.open.update((v) => !v);
    if (this.open()) {
      setTimeout(() => this.menuItems.first?.nativeElement.focus());
    } else {
      this.triggerButton?.nativeElement.focus();
    }
  }

  close(): void {
    this.open.set(false);
    this.triggerButton?.nativeElement.focus();
  }

  protected handleClick(event: MouseEvent): void {
    if (!this.open()) return;
    if (!this.elementRef.nativeElement.contains(event.target as Node)) {
      this.close();
    }
  }

  protected handleArrowDown(event: Event): void {
    if (!this.open()) return;
    event.preventDefault();
    const items = this.menuItems.toArray();
    if (items.length === 0) return;
    const currentIndex = items.findIndex((item) => item.nativeElement === document.activeElement);
    const nextIndex = currentIndex < items.length - 1 ? currentIndex + 1 : 0;
    items[nextIndex].nativeElement.focus();
  }

  protected handleArrowUp(event: Event): void {
    if (!this.open()) return;
    event.preventDefault();
    const items = this.menuItems.toArray();
    if (items.length === 0) return;
    const currentIndex = items.findIndex((item) => item.nativeElement === document.activeElement);
    const prevIndex = currentIndex > 0 ? currentIndex - 1 : items.length - 1;
    items[prevIndex].nativeElement.focus();
  }

  protected async signOut(): Promise<void> {
    this.close();
    await this.auth.logout();
  }
}
