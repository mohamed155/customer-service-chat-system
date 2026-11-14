import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  signal,
  viewChild,
  ElementRef,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { APP_PATHS } from '../../../../core/router/app-paths';
import { DashboardCardComponent } from '../../../../shared/components/dashboard-card/dashboard-card.component';
import { FormFieldComponent } from '../../../../shared/components/form-field/form-field.component';
import { InlineAlertComponent } from '../../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../../shared/components/loading-state/loading-state.component';
import { PageContainerComponent } from '../../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../../layout/page-header/page-header.component';
import { ButtonComponent } from '../../../../shared/components/button/button.component';
import { PromptStore } from './prompt.store';
import { VersionHistoryDrawerComponent } from './version-history-drawer.component';
import { VariablesPanelComponent } from './variables-panel.component';
import { PreviewPanelComponent } from './preview-panel.component';
import { validatePrompt } from './prompt-lang';
import { ApiError } from '../../../../core/api/api.models';
import { errorMessageKey } from '../../../../core/errors/error-message';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

@Component({
  selector: 'app-prompt-page',
  standalone: true,
  imports: [
    ButtonComponent,
    DashboardCardComponent,
    FormFieldComponent,
    FormsModule,
    InlineAlertComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    PreviewPanelComponent,
    RouterLink,
    VariablesPanelComponent,
    VersionHistoryDrawerComponent,
    TranslocoPipe,
  ],
  providers: [provideTranslocoScope('ai-agent'), PromptStore],
  templateUrl: './prompt-page.component.html',
  styles: [
    `
      .back-link {
        color: var(--app-accent);
        text-decoration: none;
        font-size: var(--app-font-sm);
      }
      .back-link:hover {
        text-decoration: underline;
      }
      .prompt-textarea {
        width: 100%;
        box-sizing: border-box;
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font: inherit;
        font-size: var(--app-font-sm);
        line-height: 1.6;
        resize: vertical;
        min-height: 200px;
        font-family: monospace;
      }
      .prompt-textarea:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-color: var(--app-accent);
      }
      .change-note-input {
        width: 100%;
        box-sizing: border-box;
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font: inherit;
        font-size: var(--app-font-sm);
      }
      .change-note-input:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-color: var(--app-accent);
      }
      .counter {
        display: flex;
        justify-content: flex-end;
        margin-top: var(--app-space-1);
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
      }
      .validation-issues {
        margin-top: var(--app-space-2);
        display: flex;
        flex-direction: column;
        gap: var(--app-space-1);
      }
      .actions-bar {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: var(--app-space-3);
        margin-top: var(--app-space-5);
        padding-top: var(--app-space-4);
        border-top: 1px solid var(--app-border);
      }
      .save-btn {
        height: 38px;
        padding: 0 var(--app-space-5);
        border: 0;
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 650;
        font-size: var(--app-font-sm);
        cursor: pointer;
      }
      .save-btn:disabled {
        opacity: 0.6;
        cursor: default;
      }
      .save-btn:hover:not(:disabled) {
        opacity: 0.92;
      }
      .saving-indicator {
        font-size: var(--app-font-sm);
        color: var(--app-text-2);
      }
      .version-history-bar {
        margin: var(--app-space-4) 0;
      }
      .conflict-banner,
      .notice-banner {
        margin-bottom: var(--app-space-3);
      }
      .link-btn {
        background: none;
        border: none;
        color: inherit;
        text-decoration: underline;
        cursor: pointer;
        font: inherit;
        padding: 0;
      }
      .error-state {
        padding: var(--app-space-5);
        display: grid;
        gap: var(--app-space-3);
      }
      .retry-btn {
        height: 38px;
        padding: 0 var(--app-space-5);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font-weight: 650;
        font-size: var(--app-font-sm);
        cursor: pointer;
        width: fit-content;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PromptPageComponent {
  protected readonly store = inject(PromptStore);
  protected readonly APP_PATHS = APP_PATHS;
  protected readonly versionHistoryOpen = signal(false);
  protected readonly maxContentLength = computed(
    () => this.store.bootstrap()?.limits?.maxContentLength ?? 8000,
  );
  protected readonly maxChangeNoteLength = computed(
    () => this.store.bootstrap()?.limits?.maxChangeNoteLength ?? 500,
  );
  protected readonly touched = signal(false);
  private readonly catalogNames = computed(
    () => this.store.bootstrap()?.variables?.map((v) => v.name) ?? [],
  );
  protected readonly clientIssues = computed(() =>
    validatePrompt(this.store.editorContent(), this.catalogNames()),
  );
  protected readonly visibleIssues = computed(() => {
    const all = this.clientIssues();
    if (!this.store.dirty() && !this.touched()) {
      return all.length > 0 ? all.filter((i) => i.code !== 'required') : all;
    }
    return all;
  });
  private readonly textareaRef = viewChild<ElementRef<HTMLTextAreaElement>>('promptTextarea');

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  protected fieldError(field: string): string[] | null {
    return this.store.fieldErrors()?.[field] ?? null;
  }

  protected handleInsertVariable(variableName: string): void {
    const textareaEl = this.textareaRef()?.nativeElement;
    if (!textareaEl) return;

    const start = textareaEl.selectionStart;
    const end = textareaEl.selectionEnd;
    const current = this.store.editorContent();
    const insertion = `{{${variableName}}}`;
    const newContent = current.slice(0, start) + insertion + current.slice(end);

    this.store.setContent(newContent);

    const newCursor = start + insertion.length;
    textareaEl.focus();
    textareaEl.setSelectionRange(newCursor, newCursor);
  }
}
