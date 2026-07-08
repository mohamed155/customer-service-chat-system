import { ChangeDetectionStrategy, Component, effect, inject, model, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { firstValueFrom } from 'rxjs';
import { TuiIcon } from '@taiga-ui/core';
import { ApiService } from '../../core/api/api.service';
import { CurrentUserService } from '../../core/tenant/current-user.service';
import { TenantContextService } from '../../core/tenant/tenant-context.service';
import { TenantSummary } from '../../core/api/tenant-api.models';

@Component({
  selector: 'app-tenant-switcher',
  imports: [FormsModule, TuiIcon],
  template: `
    <div class="switcher">
      <button
        type="button"
        class="trigger"
        (click)="toggle()"
        [attr.aria-expanded]="open()"
        aria-label="Switch tenant"
      >
        <tui-icon icon="@tui.building" />
        <span class="name">{{ activeTenant()?.name ?? 'Select tenant...' }}</span>
        <tui-icon icon="@tui.chevron-down" [style.rotate]="open() ? '180deg' : '0deg'" />
      </button>

      @if (open()) {
        <div
          class="dropdown"
          (click)="close()"
          (keydown.Enter)="close()"
          (keydown.Space)="$event.preventDefault(); close()"
          tabindex="-1"
          role="button"
        >
          <label class="search-label">
            <tui-icon icon="@tui.search" class="search-icon" />
            <input
              class="search-input"
              type="search"
              placeholder="Search tenants..."
              [(ngModel)]="query"
              (click)="$event.stopPropagation()"
            />
          </label>
          <div class="list">
            @for (tenant of filteredTenants(); track tenant.id) {
              <button
                type="button"
                class="option"
                [class.active]="tenant.id === activeTenant()?.id"
                (click)="select(tenant)"
              >
                <span class="option-name">{{ tenant.name }}</span>
                <span class="option-slug">{{ tenant.slug }}</span>
              </button>
            } @empty {
              <div class="empty">No tenants found</div>
            }
          </div>
        </div>
      }
    </div>
  `,
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
      .empty {
        padding: var(--app-space-4);
        text-align: center;
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TenantSwitcherComponent {
  private readonly api = inject(ApiService);
  private readonly tenantContext = inject(TenantContextService);
  protected readonly currentUser = inject(CurrentUserService);

  protected readonly open = signal(false);
  protected readonly query = model('');
  protected readonly tenants = signal<TenantSummary[]>([]);

  protected readonly activeTenant = this.tenantContext.activeTenant;
  protected readonly isPlatformUser = this.currentUser.isPlatformUser;

  protected readonly filteredTenants = signal<TenantSummary[]>([]);

  constructor() {
    this.loadTenants();
    effect(() => {
      this.query();
      this.applyFilter();
    });
  }

  async loadTenants(): Promise<void> {
    try {
      const response = await firstValueFrom(this.api.list<TenantSummary>('/platform/tenants'));
      this.tenants.set(response.data.items);
      this.applyFilter();
    } catch {
      this.tenants.set([]);
      this.filteredTenants.set([]);
    }
  }

  toggle(): void {
    if (!this.open()) {
      this.loadTenants();
    }
    this.open.update((v) => !v);
  }

  close(): void {
    this.open.set(false);
  }

  async select(tenant: TenantSummary): Promise<void> {
    this.close();
    await this.tenantContext.select(tenant.id);
  }

  private applyFilter(): void {
    const q = this.query().toLowerCase();
    const items = this.tenants();
    this.filteredTenants.set(
      q
        ? items.filter((t) => t.name.toLowerCase().includes(q) || t.slug.toLowerCase().includes(q))
        : items,
    );
  }
}
