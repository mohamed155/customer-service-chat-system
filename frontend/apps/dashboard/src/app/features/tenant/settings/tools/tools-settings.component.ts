import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { lastValueFrom } from 'rxjs';
import {
  BuiltinToolSetting,
  CreateTenantToolPayload,
  TenantDefinedTool,
  UpdateTenantToolPayload,
} from '../../../../core/api/tenant-api.models';
import { PageContainerComponent } from '../../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../../layout/page-header/page-header.component';
import { DashboardCardComponent } from '../../../../shared/components/dashboard-card/dashboard-card.component';
import { DataTableComponent } from '../../../../shared/components/data-table/data-table.component';
import { DialogShellComponent } from '../../../../shared/components/dialog-shell/dialog-shell.component';
import { FormFieldComponent } from '../../../../shared/components/form-field/form-field.component';
import { InlineAlertComponent } from '../../../../shared/components/inline-alert/inline-alert.component';
import { SectionHeaderComponent } from '../../../../shared/components/section-header/section-header.component';
import { StatusBadgeComponent } from '../../../../shared/components/status-badge/status-badge.component';
import { ToolsSettingsApiService } from './tools-settings-api.service';

@Component({
  selector: 'app-tools-settings',
  imports: [
    DashboardCardComponent,
    DataTableComponent,
    DialogShellComponent,
    FormFieldComponent,
    FormsModule,
    InlineAlertComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
  ],
  template: `
    <app-page-container>
      <app-page-header title="AI Tool Settings" description="Manage built-in and custom AI tools" />

      @if (loading()) {
        <div class="skeleton">
          <div class="shimmer"></div>
          <div class="shimmer"></div>
        </div>
      } @else if (error()) {
        <app-inline-alert tone="error">{{ error() }}</app-inline-alert>
        <button type="button" class="btn" (click)="loadTools()">Retry</button>
      } @else {
        <app-section-header
          title="Built-in tools"
          subtitle="Platform-provided tools the AI may use"
        />
        <section class="builtin-grid">
          @for (tool of builtinTools(); track tool.name) {
            <app-dashboard-card>
              <div class="tool-head">
                <strong>{{ tool.name }}</strong>
                <app-status-badge
                  [status]="tool.classification"
                  [tone]="tool.classification === 'approval' ? 'amber' : 'green'"
                />
              </div>
              <p class="tool-desc">{{ tool.description }}</p>
              <div class="tool-controls">
                <label class="toggle">
                  <input
                    type="checkbox"
                    [checked]="tool.enabled"
                    (change)="toggleBuiltinEnabled(tool.name, !tool.enabled)"
                  />
                  <span>Enabled</span>
                </label>
                <label class="toggle" [class.toggle--disabled]="tool.classification === 'approval'">
                  <input
                    type="checkbox"
                    [checked]="tool.requireApproval"
                    [disabled]="tool.classification === 'approval'"
                    (change)="toggleBuiltinApproval(tool.name, tool.enabled, !tool.requireApproval)"
                  />
                  <span>Require approval</span>
                </label>
              </div>
              @if (tool.effectiveApproval) {
                <p class="hint">Effective approval: required</p>
              }
            </app-dashboard-card>
          }
        </section>

        <div class="tenant-header">
          <app-section-header
            title="Custom tools"
            subtitle="Tools you define routed to an external HTTPS endpoint"
          />
          <button type="button" class="btn btn--primary" (click)="openCreateDialog()">
            Add tool
          </button>
        </div>

        @if (tenantDefinedTools().length === 0) {
          <p class="empty">No custom tools yet.</p>
        } @else {
          <app-data-table>
            <table>
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Description</th>
                  <th>Classification</th>
                  <th>Status</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                @for (tool of tenantDefinedTools(); track tool.id) {
                  <tr>
                    <td class="mono">{{ tool.name }}</td>
                    <td class="muted">{{ tool.description }}</td>
                    <td>
                      <app-status-badge
                        [status]="tool.classification"
                        [tone]="tool.classification === 'approval' ? 'amber' : 'green'"
                      />
                    </td>
                    <td>
                      <app-status-badge
                        [status]="tool.enabled ? 'enabled' : 'disabled'"
                        [tone]="tool.enabled ? 'green' : 'neutral'"
                      />
                    </td>
                    <td class="actions">
                      <button type="button" class="btn btn--sm" (click)="openEditDialog(tool)">
                        Edit
                      </button>
                      <button
                        type="button"
                        class="btn btn--sm btn--danger"
                        (click)="confirmDelete(tool)"
                      >
                        Delete
                      </button>
                    </td>
                  </tr>
                }
              </tbody>
            </table>
          </app-data-table>
        }
      }
    </app-page-container>

    @if (dialogOpen()) {
      <app-dialog-shell
        [open]="dialogOpen()"
        [dismissDisabled]="submitting()"
        ariaLabelledby="tool-dialog-title"
        (dismiss)="closeDialog()"
      >
        <h2 id="tool-dialog-title">
          {{ editingTool() ? 'Edit tool' : 'Add custom tool' }}
        </h2>

        @if (dialogError()) {
          <app-inline-alert tone="error">{{ dialogError() }}</app-inline-alert>
        }

        <app-form-field label="Name">
          <input
            id="tool-name"
            [(ngModel)]="formName"
            placeholder="my_custom_tool"
            [disabled]="!!editingTool()"
          />
        </app-form-field>

        <app-form-field label="Description">
          <input
            id="tool-desc"
            [(ngModel)]="formDescription"
            placeholder="Describe what this tool does"
          />
        </app-form-field>

        <app-form-field label="Input schema (JSON)">
          <textarea
            id="tool-schema"
            [(ngModel)]="formSchema"
            rows="5"
            placeholder='{"type":"object","properties":{...}}'
          ></textarea>
        </app-form-field>

        <app-form-field label="Endpoint URL">
          <input
            id="tool-url"
            [(ngModel)]="formEndpointUrl"
            placeholder="https://api.example.com/tools/..."
          />
        </app-form-field>

        <app-form-field label="Credential">
          <input
            id="tool-credential"
            type="password"
            [(ngModel)]="formCredential"
            [placeholder]="editingTool() ? 'Leave empty to keep current' : 'Optional API key'"
          />
        </app-form-field>

        <app-form-field label="Classification">
          <select id="tool-classification" [(ngModel)]="formClassification">
            <option value="approval">Approval (staff must approve)</option>
            <option value="auto">Auto (AI decides)</option>
          </select>
        </app-form-field>

        <label class="toggle">
          <input type="checkbox" [(ngModel)]="formEnabled" />
          <span>Enabled</span>
        </label>

        <div class="dialog-actions">
          <button type="button" class="btn" (click)="closeDialog()" [disabled]="submitting()">
            Cancel
          </button>
          <button
            type="button"
            class="btn btn--primary"
            (click)="submitTool()"
            [disabled]="submitting()"
          >
            {{ submitting() ? 'Saving…' : editingTool() ? 'Save changes' : 'Create tool' }}
          </button>
        </div>
      </app-dialog-shell>
    }

    @if (deleteConfirmTool()) {
      <app-dialog-shell
        [open]="!!deleteConfirmTool()"
        [dismissDisabled]="submitting()"
        ariaLabelledby="delete-dialog-title"
        (dismiss)="deleteConfirmTool.set(null)"
      >
        <h2 id="delete-dialog-title">Delete tool</h2>
        <p>
          Are you sure you want to delete <strong>{{ deleteConfirmTool()?.name }}</strong
          >? Existing tool requests referencing it will remain intact.
        </p>
        <div class="dialog-actions">
          <button
            type="button"
            class="btn"
            (click)="deleteConfirmTool.set(null)"
            [disabled]="submitting()"
          >
            Cancel
          </button>
          <button
            type="button"
            class="btn btn--danger"
            (click)="executeDelete()"
            [disabled]="submitting()"
          >
            {{ submitting() ? 'Deleting…' : 'Delete' }}
          </button>
        </div>
      </app-dialog-shell>
    }
  `,
  styles: [
    `
      .skeleton {
        display: grid;
        gap: var(--app-space-4);
      }
      .shimmer {
        height: 80px;
        border-radius: var(--app-radius-lg);
        background: var(--app-panel-2);
      }
      .builtin-grid {
        display: grid;
        gap: var(--app-space-4);
        grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
        margin-bottom: var(--app-space-8);
      }
      .tool-head {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: var(--app-space-2);
      }
      .tool-head strong {
        color: var(--app-text);
        font-size: var(--app-font-base);
      }
      .tool-desc {
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        margin: 0 0 var(--app-space-4);
        line-height: 1.5;
      }
      .tool-controls {
        display: flex;
        gap: var(--app-space-4);
      }
      .toggle {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 600;
        cursor: pointer;
      }
      .toggle--disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
      .toggle--disabled input {
        pointer-events: none;
      }
      .hint {
        margin: var(--app-space-2) 0 0;
        font-size: var(--app-font-xs);
        color: var(--app-amber);
        font-weight: 600;
      }
      .tenant-header {
        display: flex;
        justify-content: space-between;
        align-items: start;
        gap: var(--app-space-4);
        margin-bottom: var(--app-space-4);
      }
      .empty {
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
        margin: var(--app-space-4) 0;
      }
      .mono {
        font-family: var(--app-font-mono);
        font-weight: 600;
      }
      .actions {
        display: flex;
        gap: var(--app-space-2);
        justify-content: end;
      }
      .btn {
        height: 36px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font-weight: 650;
        cursor: pointer;
        font: inherit;
      }
      .btn--primary {
        background: var(--app-accent);
        color: var(--app-white, #fff);
        border-color: var(--app-accent);
      }
      .btn--danger {
        background: var(--app-red);
        color: var(--app-white, #fff);
        border-color: var(--app-red);
      }
      .btn--sm {
        height: 30px;
        padding: 0 var(--app-space-2);
        font-size: var(--app-font-sm);
      }
      .btn:disabled {
        opacity: 0.6;
        cursor: default;
      }
      textarea {
        width: 100%;
        box-sizing: border-box;
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font: inherit;
        font-family: var(--app-font-mono);
        font-size: var(--app-font-sm);
        resize: vertical;
      }
      textarea:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-color: var(--app-accent);
      }
      select {
        width: 100%;
        box-sizing: border-box;
      }
      .dialog-actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--app-space-2);
        margin-top: var(--app-space-5);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolsSettingsComponent {
  private readonly api = inject(ToolsSettingsApiService);

  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);
  protected readonly builtinTools = signal<BuiltinToolSetting[]>([]);
  protected readonly tenantDefinedTools = signal<TenantDefinedTool[]>([]);

  protected readonly dialogOpen = signal(false);
  protected readonly editingTool = signal<TenantDefinedTool | null>(null);
  protected readonly submitting = signal(false);
  protected readonly dialogError = signal<string | null>(null);

  protected readonly deleteConfirmTool = signal<TenantDefinedTool | null>(null);

  protected formName = '';
  protected formDescription = '';
  protected formSchema = '';
  protected formEndpointUrl = '';
  protected formCredential = '';
  protected formClassification: 'auto' | 'approval' = 'approval';
  protected formEnabled = true;

  constructor() {
    this.loadTools();
  }

  protected loadTools(): void {
    this.loading.set(true);
    this.error.set(null);
    lastValueFrom(this.api.getTools()).then(
      (result) => {
        this.builtinTools.set([...result.builtin]);
        this.tenantDefinedTools.set([...result.tenantDefined]);
        this.loading.set(false);
      },
      (err: unknown) => {
        this.error.set(err instanceof Error ? err.message : 'Failed to load tools');
        this.loading.set(false);
      },
    );
  }

  protected toggleBuiltinEnabled(name: string, enabled: boolean): void {
    const tool = this.builtinTools().find((t) => t.name === name);
    if (!tool) return;
    lastValueFrom(this.api.updateBuiltinPolicy(name, enabled, tool.requireApproval)).then(
      () => {
        this.builtinTools.update((list) =>
          list.map((t) => (t.name === name ? { ...t, enabled } : t)),
        );
      },
      () => {
        this.error.set(`Failed to update ${name}`);
      },
    );
  }

  protected toggleBuiltinApproval(name: string, enabled: boolean, requireApproval: boolean): void {
    const tool = this.builtinTools().find((t) => t.name === name);
    if (!tool || tool.classification === 'approval') return;
    lastValueFrom(this.api.updateBuiltinPolicy(name, enabled, requireApproval)).then(
      () => {
        this.builtinTools.update((list) =>
          list.map((t) =>
            t.name === name ? { ...t, requireApproval, effectiveApproval: requireApproval } : t,
          ),
        );
      },
      () => {
        this.error.set(`Failed to update ${name}`);
      },
    );
  }

  protected openCreateDialog(): void {
    this.editingTool.set(null);
    this.resetForm();
    this.dialogError.set(null);
    this.dialogOpen.set(true);
  }

  protected openEditDialog(tool: TenantDefinedTool): void {
    this.editingTool.set(tool);
    this.formName = tool.name;
    this.formDescription = tool.description;
    this.formSchema = JSON.stringify(tool.inputSchema, null, 2);
    this.formEndpointUrl = tool.endpointUrl;
    this.formCredential = '';
    this.formClassification = tool.classification;
    this.formEnabled = tool.enabled;
    this.dialogError.set(null);
    this.dialogOpen.set(true);
  }

  protected closeDialog(): void {
    this.dialogOpen.set(false);
    this.submitting.set(false);
  }

  protected submitTool(): void {
    if (this.submitting()) return;

    let inputSchema: Record<string, unknown>;
    try {
      inputSchema = JSON.parse(this.formSchema || '{}');
    } catch {
      this.dialogError.set('Input schema is not valid JSON');
      return;
    }

    this.submitting.set(true);
    this.dialogError.set(null);

    const editing = this.editingTool();
    if (editing) {
      const payload: UpdateTenantToolPayload = {
        name: this.formName,
        description: this.formDescription,
        inputSchema,
        endpointUrl: this.formEndpointUrl,
        classification: this.formClassification,
        enabled: this.formEnabled,
        ...(this.formCredential ? { credential: this.formCredential } : {}),
      };
      lastValueFrom(this.api.updateTenantTool(editing.id, payload)).then(
        (updated) => {
          this.tenantDefinedTools.update((list) =>
            list.map((t) => (t.id === updated.id ? updated : t)),
          );
          this.closeDialog();
        },
        (err: unknown) => {
          this.dialogError.set(err instanceof Error ? err.message : 'Failed to update tool');
          this.submitting.set(false);
        },
      );
    } else {
      const payload: CreateTenantToolPayload = {
        name: this.formName,
        description: this.formDescription,
        inputSchema,
        endpointUrl: this.formEndpointUrl,
        credential: this.formCredential || null,
        classification: this.formClassification,
        enabled: this.formEnabled,
      };
      lastValueFrom(this.api.createTenantTool(payload)).then(
        (created) => {
          this.tenantDefinedTools.update((list) => [...list, created]);
          this.closeDialog();
        },
        (err: unknown) => {
          this.dialogError.set(err instanceof Error ? err.message : 'Failed to create tool');
          this.submitting.set(false);
        },
      );
    }
  }

  protected confirmDelete(tool: TenantDefinedTool): void {
    this.deleteConfirmTool.set(tool);
  }

  protected executeDelete(): void {
    const tool = this.deleteConfirmTool();
    if (!tool || this.submitting()) return;

    this.submitting.set(true);
    lastValueFrom(this.api.deleteTenantTool(tool.id)).then(
      () => {
        this.tenantDefinedTools.update((list) => list.filter((t) => t.id !== tool.id));
        this.deleteConfirmTool.set(null);
        this.submitting.set(false);
      },
      () => {
        this.error.set('Failed to delete tool');
        this.submitting.set(false);
        this.deleteConfirmTool.set(null);
      },
    );
  }

  private resetForm(): void {
    this.formName = '';
    this.formDescription = '';
    this.formSchema = '';
    this.formEndpointUrl = '';
    this.formCredential = '';
    this.formClassification = 'approval';
    this.formEnabled = true;
  }
}
