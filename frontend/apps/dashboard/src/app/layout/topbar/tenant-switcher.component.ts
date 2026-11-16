import {
  afterNextRender,
  ChangeDetectionStrategy,
  Component,
  computed,
  ElementRef,
  inject,
  Injector,
  signal,
  viewChild,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { catchError, EMPTY, finalize, map, pipe, Subject, switchMap, takeUntil, tap } from 'rxjs';
import { TuiIcon } from '@taiga-ui/core';
import { ApiService } from '../../core/api/api.service';
import { CurrentUserService } from '../../core/tenant/current-user.service';
import { TenantContextService } from '../../core/tenant/tenant-context.service';
import { TenantSummary } from '../../core/api/tenant-api.models';
import { TranslocoPipe } from '@jsverse/transloco';

let nextId = 0;

/**
 * The platform operator's workspace picker: a button that opens a searchable list.
 *
 * Keyboard (WAI-ARIA combobox with a listbox popup): the search field keeps focus and names the
 * highlighted option with `aria-activedescendant`. ArrowDown/ArrowUp move the highlight, Home/End
 * jump to the ends (they steer the list, not the caret, in this short field), Enter picks,
 * Escape closes. Focus goes back to the button after Escape, a click outside and a selection.
 *
 * Loading and switching are RxJS pipelines: a new load replaces the one in flight, closing the
 * list cancels it, and the list is never overwritten by an answer that arrives late.
 */
@Component({
  selector: 'app-tenant-switcher',
  imports: [FormsModule, TuiIcon, TranslocoPipe],
  templateUrl: './tenant-switcher.component.html',
  styles: [
    `
      .switcher {
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
      .name {
        max-width: 140px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: var(--app-font-sm);
      }
      .dropdown {
        position: absolute;
        top: calc(100% + 4px);
        right: 0;
        width: 280px;
        background: var(--app-panel);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        box-shadow: var(--app-shadow-lg);
        z-index: 100;
        overflow: hidden;
      }
      .search-label {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        padding: var(--app-space-2);
        border-bottom: 1px solid var(--app-border);
      }
      .search-icon {
        font-size: 16px;
        color: var(--app-text-3);
        flex-shrink: 0;
      }
      .search-input {
        flex: 1;
        border: 0;
        outline: 0;
        background: transparent;
        color: var(--app-text);
        font: inherit;
      }
      .search-input::placeholder {
        color: var(--app-text-3);
      }
      .list {
        max-height: 240px;
        overflow-y: auto;
      }
      .option {
        width: 100%;
        box-sizing: border-box;
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
      }
      .option:hover {
        background: var(--app-fill-hover);
      }
      .option.active {
        background: var(--app-accent-soft);
        color: var(--app-accent);
      }
      .option.highlighted {
        outline: 2px solid var(--app-accent);
        outline-offset: -2px;
      }
      .option-name {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: var(--app-font-sm);
        font-weight: 500;
      }
      .option-slug {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .error {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-2);
        padding: var(--app-space-2) var(--app-space-3);
        border-bottom: 1px solid var(--app-border);
        background: var(--app-red-soft);
        color: var(--app-red);
        font-size: var(--app-font-sm);
      }
      .error button {
        border: 0;
        background: transparent;
        color: inherit;
        font: inherit;
        font-weight: 650;
        text-decoration: underline;
        cursor: pointer;
      }
      .empty {
        padding: var(--app-space-4);
        text-align: center;
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
      }
    `,
  ],
  host: {
    '(document:click)': 'handleClick($event)',
    '(keydown.escape)': 'onEscape($event)',
    '(focusout)': 'onFocusOut($event)',
  },
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TenantSwitcherComponent {
  private readonly api = inject(ApiService);
  private readonly tenantContext = inject(TenantContextService);
  private readonly elementRef = inject<ElementRef<HTMLElement>>(ElementRef);
  private readonly injector = inject(Injector);
  protected readonly currentUser = inject(CurrentUserService);

  /** Prefix of the element ids, so two switchers on a page never share one. */
  protected readonly uid = `tenant-switcher-${nextId++}`;
  protected readonly listId = `${this.uid}-list`;

  protected readonly open = signal(false);
  protected readonly query = signal('');
  protected readonly tenants = signal<TenantSummary[]>([]);
  protected readonly loading = signal(false);
  protected readonly loadFailed = signal(false);
  protected readonly switchFailed = signal(false);

  protected readonly activeTenant = this.tenantContext.activeTenant;
  protected readonly isPlatformUser = this.currentUser.isPlatformUser;

  protected readonly filteredTenants = computed(() => {
    const q = this.query().toLowerCase();
    const items = this.tenants();
    return q
      ? items.filter((t) => t.name.toLowerCase().includes(q) || t.slug.toLowerCase().includes(q))
      : items;
  });

  /** Where the person moved the highlight; `null` until they do (it then starts on the active tenant). */
  private readonly moved = signal<number | null>(null);

  /** The index of the highlighted option: the one moved to, else the active tenant, else the first. */
  protected readonly highlightedIndex = computed(() => {
    const items = this.filteredTenants();
    if (items.length === 0) return -1;
    const moved = this.moved();
    if (moved !== null) return Math.min(moved, items.length - 1);
    const active = items.findIndex((t) => t.id === this.activeTenant()?.id);
    return active >= 0 ? active : 0;
  });

  protected readonly highlightedId = computed(() => {
    const tenant = this.filteredTenants()[this.highlightedIndex()];
    return tenant ? this.optionId(tenant) : null;
  });

  private readonly trigger = viewChild<ElementRef<HTMLButtonElement>>('trigger');
  private readonly searchInput = viewChild<ElementRef<HTMLInputElement>>('searchInput');
  private readonly list = viewChild<ElementRef<HTMLElement>>('list');

  /** Emits when the list closes: whatever is still loading for it is no longer wanted. */
  private readonly closed$ = new Subject<void>();

  /** Loads the tenants; asking again replaces the request in flight, closing cancels it. */
  private readonly load = rxMethod<void>(
    pipe(
      switchMap(() => {
        this.loading.set(true);
        this.loadFailed.set(false);
        return this.api.list<TenantSummary>('/platform/tenants').pipe(
          map((response) => response.data.items),
          tap((items) => this.tenants.set(items)),
          catchError(() => {
            this.loadFailed.set(true);
            return EMPTY;
          }),
          takeUntil(this.closed$),
          finalize(() => this.loading.set(false)),
        );
      }),
    ),
  );

  /** Switches to the picked tenant; picking another, or closing the list, abandons a switch still in flight. */
  private readonly choose = rxMethod<TenantSummary>(
    pipe(
      switchMap((tenant) => {
        this.switchFailed.set(false);
        return this.tenantContext.select$(tenant.id).pipe(
          tap(() => {
            if (this.open()) this.closeAndFocusTrigger();
          }),
          catchError(() => {
            this.switchFailed.set(true);
            return EMPTY;
          }),
          // Closing the list abandons a switch still on its way: it must not fail unseen.
          takeUntil(this.closed$),
        );
      }),
    ),
  );

  toggle(): void {
    if (this.open()) {
      this.close();
    } else {
      this.openList();
    }
  }

  /** Closes the list and cancels what it was loading. Focus stays where it is. */
  close(): void {
    if (!this.open()) return;
    this.open.set(false);
    this.closed$.next();
  }

  protected openList(): void {
    this.query.set('');
    this.moved.set(null);
    this.switchFailed.set(false);
    this.open.set(true);
    this.load();
    afterNextRender(() => this.searchInput()?.nativeElement.focus(), { injector: this.injector });
  }

  protected retry(): void {
    this.load();
  }

  private closeAndFocusTrigger(): void {
    this.close();
    this.trigger()?.nativeElement.focus();
  }

  protected optionId(tenant: TenantSummary): string {
    return `${this.uid}-option-${tenant.id}`;
  }

  protected onQueryChange(value: string): void {
    this.query.set(value);
    this.moved.set(null);
  }

  protected onTriggerKeydown(event: KeyboardEvent): void {
    if (!this.open() && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
      event.preventDefault();
      this.openList();
    }
  }

  protected onSearchKeydown(event: KeyboardEvent): void {
    const count = this.filteredTenants().length;
    const current = this.highlightedIndex();
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        if (count) this.moveTo((current + 1) % count);
        break;
      case 'ArrowUp':
        event.preventDefault();
        if (count) this.moveTo((current - 1 + count) % count);
        break;
      case 'Home':
        event.preventDefault();
        if (count) this.moveTo(0);
        break;
      case 'End':
        event.preventDefault();
        if (count) this.moveTo(count - 1);
        break;
      case 'Enter': {
        event.preventDefault();
        const tenant = this.filteredTenants()[current];
        if (tenant) this.select(tenant);
        break;
      }
    }
  }

  private moveTo(index: number): void {
    this.moved.set(index);
    // The highlighted option may be below the fold of the list: bring it into view.
    afterNextRender(
      () =>
        this.list()
          ?.nativeElement.querySelector('.highlighted')
          ?.scrollIntoView?.({ block: 'nearest' }),
      { injector: this.injector },
    );
  }

  protected onEscape(event: Event): void {
    if (!this.open()) return;
    // A search field clears itself on Escape; here Escape closes the list.
    event.preventDefault();
    this.closeAndFocusTrigger();
  }

  /** A click outside closes the list; focus goes back to the button unless the click focused something. */
  protected handleClick(event: MouseEvent): void {
    if (!this.open()) return;
    if (this.elementRef.nativeElement.contains(event.target as Node)) return;
    this.close();
    const active = this.elementRef.nativeElement.ownerDocument.activeElement;
    if (!active || active === this.elementRef.nativeElement.ownerDocument.body) {
      this.trigger()?.nativeElement.focus();
    }
  }

  /** Tabbing out of the switcher closes the list; focus is already where the person sent it. */
  protected onFocusOut(event: FocusEvent): void {
    if (!this.open()) return;
    const next = event.relatedTarget as Node | null;
    if (next && !this.elementRef.nativeElement.contains(next)) this.close();
  }

  select(tenant: TenantSummary): void {
    this.choose(tenant);
  }
}
