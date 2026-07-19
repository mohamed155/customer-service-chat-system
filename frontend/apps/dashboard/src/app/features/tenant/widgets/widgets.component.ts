import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { DatePipe } from '@angular/common';
import { TuiIcon } from '@taiga-ui/core';
import { WidgetInstance } from '../../../core/api/widget.models';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { WidgetPreviewComponent } from './widget-preview.component';
import { WidgetEditorComponent } from './widget-editor.component';
import { WidgetsStore } from './widgets.store';

@Component({
  selector: 'app-widgets',
  standalone: true,
  imports: [
    ButtonComponent,
    DashboardCardComponent,
    DatePipe,
    EmptyStateComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    TuiIcon,
    WidgetEditorComponent,
    WidgetPreviewComponent,
  ],
  providers: [WidgetsStore],
  template: `
    <app-page-container>
      <app-page-header title="Chat Widget" description="Embed the chat widget on your website">
        @if (canManage()) {
          <button type="button" class="new-btn" (click)="startCreate()">
            <tui-icon icon="@tui.plus" />
            New Widget
          </button>
        }
      </app-page-header>

      @if (store.loading() && !store.instances().length) {
        <app-loading-state />
      } @else if (store.error(); as err) {
        <app-empty-state icon="@tui.alert-circle" title="Something went wrong" [description]="err">
          <button type="button" (click)="store.loadList()">Try again</button>
        </app-empty-state>
      } @else {
        <div class="layout">
          <div class="list-col">
            @if (store.hasInstances()) {
              <section class="instance-list">
                @for (instance of store.instances(); track instance.id) {
                  <app-dashboard-card>
                    <div
                      class="instance-head"
                      (click)="selectInstance(instance)"
                      role="button"
                      tabindex="0"
                      (keydown.enter)="selectInstance(instance)"
                      (keydown.space)="selectInstance(instance); $event.preventDefault()"
                    >
                      <div class="instance-color" [style.background]="instance.primaryColor"></div>
                      <div>
                        <strong>{{ instance.displayName || instance.name }}</strong>
                        <span class="instance-meta">
                          {{ instance.enabled ? 'Enabled' : 'Disabled' }}
                          · Updated {{ instance.updatedAt | date: 'mediumDate' }}
                        </span>
                      </div>
                    </div>
                    <div class="card-actions" card-footer>
                      <button
                        type="button"
                        class="action-link"
                        (click)="selectInstance(instance)"
                        (keydown.enter)="selectInstance(instance)"
                      >
                        <tui-icon icon="@tui.edit" />
                        Edit
                      </button>
                      @if (canManage()) {
                        <button
                          type="button"
                          class="action-link danger"
                          (click)="confirmDelete(instance)"
                          (keydown.enter)="confirmDelete(instance)"
                        >
                          <tui-icon icon="@tui.trash-2" />
                          Delete
                        </button>
                      }
                    </div>
                  </app-dashboard-card>
                }
              </section>
            } @else {
              <app-empty-state
                icon="@tui.message-square"
                title="No widgets yet"
                description="Create your first widget to embed on your website."
              />
            }
          </div>

          <div class="detail-col">
            @if (editing(); as edit) {
              <app-widget-editor
                [form]="store.formState() || {}"
                [isNew]="edit.isNew"
                [visible]="true"
                [saving]="store.saving()"
                [error]="store.error()"
                (save)="handleSave()"
                (dismissed)="dismissEditor()"
                (formChange)="store.updateFormState($event)"
              />
            }

            @if (store.selectedId()) {
              <div class="snippet-section">
                <h4>Embed snippet</h4>
                @if (store.snippet(); as snippet) {
                  <div class="snippet-box">
                    <code>{{ snippet }}</code>
                    <button
                      type="button"
                      class="copy-btn"
                      (click)="copySnippet(snippet)"
                      [attr.aria-label]="copied() ? 'Copied' : 'Copy snippet'"
                    >
                      <tui-icon [icon]="copied() ? '@tui.check' : '@tui.copy'" />
                      {{ copied() ? 'Copied' : 'Copy' }}
                    </button>
                  </div>
                } @else {
                  <p class="snippet-placeholder">Select a widget to see the embed snippet.</p>
                }
              </div>
            }

            <app-widget-preview [formState]="store.formState() || {}" />
          </div>
        </div>
      }
    </app-page-container>

    @if (deleteTarget(); as target) {
      <div
        class="modal-overlay"
        (click)="cancelDelete()"
        role="presentation"
        (keydown.enter)="cancelDelete()"
        (keydown.escape)="cancelDelete()"
      >
        <div
          class="modal"
          (click)="$event.stopPropagation()"
          role="dialog"
          aria-modal="true"
          aria-labelledby="delete-title"
          (keydown.enter)="$event.stopPropagation()"
        >
          <h3 id="delete-title">Delete "{{ target.displayName || target.name }}"</h3>
          <p>
            This action cannot be undone. The widget will be removed and the embed snippet will stop
            working.
          </p>
          <div class="modal-actions">
            <app-button variant="secondary" size="sm" (pressed)="cancelDelete()">Cancel</app-button>
            <app-button variant="primary" size="sm" (pressed)="executeDelete()">
              {{ store.saving() ? 'Deleting…' : 'Delete' }}
            </app-button>
          </div>
        </div>
      </div>
    }
  `,
  styles: [
    `
      .new-btn {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-2);
        height: 38px;
        padding: 0 var(--app-space-4);
        border: 1px solid var(--app-accent);
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 650;
        font-size: var(--app-font-sm);
        text-decoration: none;
        cursor: pointer;
      }
      .new-btn:hover {
        opacity: 0.92;
      }
      .layout {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: var(--app-space-4);
        align-items: start;
      }
      .instance-list {
        display: grid;
        gap: var(--app-space-3);
      }
      .instance-head {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        cursor: pointer;
      }
      .instance-color {
        width: 12px;
        height: 12px;
        border-radius: 50%;
        flex-shrink: 0;
      }
      strong {
        display: block;
        color: var(--app-text);
      }
      .instance-meta {
        display: block;
        margin-top: 2px;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .card-actions {
        display: flex;
        gap: var(--app-space-2);
      }
      .action-link {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-1);
        height: 30px;
        padding: 0 var(--app-space-3);
        border: none;
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        font-weight: 650;
        text-decoration: none;
        cursor: pointer;
      }
      .action-link:hover {
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      .action-link.danger {
        color: var(--app-red, #e53935);
      }
      .action-link.danger:hover {
        background: rgba(229, 57, 53, 0.1);
        color: var(--app-red, #e53935);
      }
      .detail-col {
        display: grid;
        gap: var(--app-space-4);
      }
      .snippet-section {
        padding: var(--app-space-4);
        background: var(--app-panel);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-xl);
      }
      h4 {
        margin: 0 0 var(--app-space-2);
        color: var(--app-text);
        font-size: var(--app-font-sm);
      }
      .snippet-box {
        display: flex;
        gap: var(--app-space-2);
        align-items: flex-start;
      }
      .snippet-box code {
        flex: 1;
        padding: var(--app-space-3);
        background: var(--app-panel-2);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        font-size: var(--app-font-xs);
        color: var(--app-text);
        word-break: break-all;
        line-height: 1.5;
      }
      .copy-btn {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-1);
        height: 34px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font-size: var(--app-font-xs);
        font-weight: 650;
        cursor: pointer;
        white-space: nowrap;
        flex-shrink: 0;
      }
      .copy-btn:hover {
        background: var(--app-panel-2);
      }
      .snippet-placeholder {
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
        margin: 0;
      }
      .modal-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.4);
        display: grid;
        place-items: center;
        z-index: 1000;
      }
      .modal {
        width: 420px;
        max-width: 90vw;
        padding: var(--app-space-6);
        background: var(--app-panel);
        border-radius: var(--app-radius-xl);
        box-shadow: 0 24px 48px rgba(0, 0, 0, 0.2);
      }
      .modal h3 {
        margin: 0 0 var(--app-space-2);
        color: var(--app-text);
      }
      .modal p {
        margin: 0 0 var(--app-space-4);
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        line-height: 1.5;
      }
      .modal-actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--app-space-2);
      }
      @media (max-width: 900px) {
        .layout {
          grid-template-columns: 1fr;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class WidgetsComponent {
  protected readonly store = inject(WidgetsStore);
  private readonly permissions = inject(PermissionsService);

  protected readonly editing = signal<{ isNew: boolean } | null>(null);
  protected readonly deleteTarget = signal<WidgetInstance | null>(null);
  protected readonly copied = signal(false);

  protected readonly canManage = () => this.permissions.has('widgets.manage');

  protected startCreate(): void {
    this.store.updateFormState({
      name: '',
      displayName: '',
      primaryColor: '#0066FF',
      welcomeMessage: '',
      position: 'bottom-right',
      theme: 'light',
      enabled: true,
      allowedDomains: [],
    });
    this.editing.set({ isNew: true });
  }

  protected selectInstance(instance: WidgetInstance): void {
    this.store.selectInstance(instance.id);
    this.editing.set({ isNew: false });
  }

  protected dismissEditor(): void {
    this.editing.set(null);
    this.store.selectInstance(null);
  }

  protected handleSave(): void {
    const edit = this.editing();
    if (!edit) return;
    const form = this.store.formState();
    if (!form) return;

    if (edit.isNew) {
      this.store.createInstance({
        name: form.name || '',
        displayName: form.displayName,
        primaryColor: form.primaryColor,
        welcomeMessage: form.welcomeMessage,
        position: form.position,
        theme: form.theme,
        enabled: form.enabled,
        allowedDomains: form.allowedDomains,
      });
      this.editing.set(null);
    } else {
      const selectedId = this.store.selectedId();
      if (selectedId) {
        this.store.updateInstance(selectedId, {
          name: form.name,
          displayName: form.displayName,
          primaryColor: form.primaryColor,
          welcomeMessage: form.welcomeMessage,
          position: form.position,
          theme: form.theme,
          enabled: form.enabled,
          allowedDomains: form.allowedDomains,
        });
      }
    }
  }

  protected confirmDelete(instance: WidgetInstance): void {
    this.deleteTarget.set(instance);
  }

  protected cancelDelete(): void {
    this.deleteTarget.set(null);
  }

  protected executeDelete(): void {
    const target = this.deleteTarget();
    if (!target) return;
    this.store.deleteInstance(target.id);
    this.deleteTarget.set(null);
    this.editing.set(null);
  }

  protected copySnippet(snippet: string): void {
    navigator.clipboard.writeText(snippet).then(() => {
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 2000);
    });
  }
}
