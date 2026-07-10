import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { Store } from '@ngrx/store';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import {
  API_KEY_FIXTURE,
  INVOICE_FIXTURES,
  SESSION_FIXTURES,
  TEAM_MEMBERS,
  USAGE_FIXTURES,
  WORKSPACE_PROFILE,
} from '../../../shared/fixtures/settings.fixtures';
import { appUiActions, ThemeMode } from '../../../core/state/app-ui.feature';
import { SettingsStore, SettingsTab } from './settings.store';

@Component({
  selector: 'app-settings',
  imports: [
    AvatarComponent,
    DashboardCardComponent,
    DataTableComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
  ],
  providers: [SettingsStore],
  template: `
    <app-page-container>
      <app-page-header title="Settings" [description]="'Workspace preferences and security'" />
      <div class="tabs" role="tablist" aria-label="Settings sections">
        @for (tab of tabs; track tab.id) {
          <button
            type="button"
            role="tab"
            [class.active]="store.activeTab() === tab.id"
            [attr.aria-selected]="store.activeTab() === tab.id"
            (click)="store.setTab(tab.id)"
          >
            {{ tab.label }}
          </button>
        }
      </div>

      @switch (store.activeTab()) {
        @case ('general') {
          <section class="grid two">
            <app-dashboard-card>
              <app-section-header
                card-header
                title="Workspace profile"
                subtitle="Static workspace preferences"
              />
              <label>Name<input [value]="profile.name" /></label>
              <label>Domain<input [value]="profile.domain" /></label>
              <label>Timezone<input [value]="profile.timezone" /></label>
              <label>Default language<input [value]="profile.defaultLanguage" /></label>
            </app-dashboard-card>
            <app-dashboard-card>
              <app-section-header
                card-header
                title="Theme preference"
                subtitle="Uses the global app UI action"
              />
              <div class="segmented">
                @for (mode of themeModes; track mode) {
                  <button type="button" (click)="setTheme(mode)">{{ mode }}</button>
                }
              </div>
              <label class="check"><input type="checkbox" checked /> Email notifications</label>
              <label class="check"><input type="checkbox" checked /> AI quality alerts</label>
            </app-dashboard-card>
          </section>
        }
        @case ('team') {
          <app-data-table>
            <table>
              <thead>
                <tr>
                  <th>Member</th>
                  <th>Role</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                @for (member of team; track member.id) {
                  <tr>
                    <td>
                      <div class="person">
                        <app-avatar [initials]="member.avatarInitials" size="sm" /><span
                          >{{ member.name }}<small>{{ member.email }}</small></span
                        >
                      </div>
                    </td>
                    <td><app-status-badge [status]="member.role" tone="accent" /></td>
                    <td>
                      <app-status-badge
                        [status]="member.status"
                        [tone]="member.status === 'active' ? 'green' : 'amber'"
                      />
                    </td>
                  </tr>
                }
              </tbody>
            </table>
          </app-data-table>
        }
        @case ('billing') {
          <section class="grid two">
            <app-dashboard-card>
              <app-section-header card-header title="Usage" subtitle="Current plan limits" />
              @for (usage of usageFixtures; track usage.label) {
                <div class="usage">
                  <span>{{ usage.label }}</span
                  ><strong>{{ usage.used }} / {{ usage.limit }} {{ usage.unit }}</strong>
                  <div><i [style.width.%]="(usage.used / usage.limit) * 100"></i></div>
                </div>
              }
            </app-dashboard-card>
            <app-data-table>
              <table>
                <thead>
                  <tr>
                    <th>Invoice</th>
                    <th>Amount</th>
                    <th>Status</th>
                  </tr>
                </thead>
                <tbody>
                  @for (invoice of invoices; track invoice.id) {
                    <tr>
                      <td>{{ invoice.period }}</td>
                      <td>{{ invoice.amount }}</td>
                      <td>
                        <app-status-badge
                          [status]="invoice.status"
                          [tone]="invoice.status === 'paid' ? 'green' : 'amber'"
                        />
                      </td>
                    </tr>
                  }
                </tbody>
              </table>
            </app-data-table>
          </section>
        }
        @case ('api-keys') {
          <app-dashboard-card>
            <app-section-header card-header title="API keys" subtitle="Masked fixture value only" />
            <label
              >{{ apiKey.label }}<input class="mono" [value]="apiKey.maskedValue" readonly
            /></label>
            <p>Created {{ apiKey.createdAt }}</p>
          </app-dashboard-card>
        }
        @case ('security') {
          <section class="grid two">
            <app-dashboard-card>
              <app-section-header
                card-header
                title="Two-factor authentication"
                subtitle="Visual switch"
              />
              <label class="switch"><input type="checkbox" checked /><span></span>Enabled</label>
            </app-dashboard-card>
            <app-data-table>
              <table>
                <thead>
                  <tr>
                    <th>Device</th>
                    <th>Location</th>
                    <th>Last active</th>
                  </tr>
                </thead>
                <tbody>
                  @for (session of sessions; track session.id) {
                    <tr>
                      <td>
                        {{ session.device }}
                        @if (session.current) {
                          <span class="current">Current</span>
                        }
                      </td>
                      <td>{{ session.location }}</td>
                      <td class="muted">{{ session.lastActiveAt }}</td>
                    </tr>
                  }
                </tbody>
              </table>
            </app-data-table>
          </section>
        }
      }
    </app-page-container>
  `,
  styles: [
    `
      .tabs,
      .segmented {
        display: flex;
        gap: var(--app-space-1);
        margin-bottom: var(--app-space-5);
        padding: 4px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
        width: fit-content;
      }
      button {
        height: 34px;
        padding: 0 var(--app-space-3);
        border: 0;
        border-radius: var(--app-radius-md);
        background: transparent;
        color: var(--app-text-2);
        font-weight: 650;
        cursor: pointer;
      }
      button.active,
      .segmented button:hover {
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      .grid {
        display: grid;
        gap: var(--app-space-4);
      }
      .two {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }
      label {
        display: grid;
        gap: 6px;
        margin-bottom: var(--app-space-3);
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        font-weight: 650;
      }
      input {
        height: 38px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
        font: inherit;
      }
      .mono {
        font-family: var(--app-font-mono);
      }
      .check,
      .switch {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
      }
      .check input,
      .switch input {
        width: auto;
        height: auto;
      }
      .person {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
      }
      small {
        display: block;
        color: var(--app-text-3);
      }
      .usage {
        display: grid;
        gap: 7px;
        margin-bottom: var(--app-space-4);
      }
      .usage span {
        color: var(--app-text-2);
      }
      .usage div {
        height: 8px;
        overflow: hidden;
        border-radius: 999px;
        background: var(--app-panel-3);
      }
      .usage i {
        display: block;
        height: 100%;
        background: var(--app-accent);
      }
      .current {
        margin-left: 8px;
        color: var(--app-green);
        font-size: var(--app-font-xs);
        font-weight: 700;
      }
      @media (max-width: 900px) {
        .two {
          grid-template-columns: 1fr;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SettingsComponent {
  private readonly globalStore = inject(Store);
  protected readonly store = inject(SettingsStore);
  protected readonly profile = WORKSPACE_PROFILE;
  protected readonly team = TEAM_MEMBERS;
  protected readonly usageFixtures = USAGE_FIXTURES;
  protected readonly invoices = INVOICE_FIXTURES;
  protected readonly apiKey = API_KEY_FIXTURE;
  protected readonly sessions = SESSION_FIXTURES;
  protected readonly themeModes: readonly ThemeMode[] = ['light', 'dark', 'system'];
  protected readonly tabs: readonly { id: SettingsTab; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'team', label: 'Team' },
    { id: 'billing', label: 'Billing' },
    { id: 'api-keys', label: 'API Keys' },
    { id: 'security', label: 'Security' },
  ];

  protected setTheme(themeMode: ThemeMode): void {
    this.globalStore.dispatch(appUiActions.themeModeChanged({ themeMode }));
  }
}
