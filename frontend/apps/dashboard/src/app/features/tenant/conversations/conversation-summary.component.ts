import { ApiError } from '../../../core/api/api.models';
import { toApiError } from '../../../core/errors/api-error';
import { errorMessageKey } from '../../../core/errors/error-message';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { ChangeDetectionStrategy, Component, inject, input, signal } from '@angular/core';
import { catchError, finalize, of } from 'rxjs';
import { ConversationsApiService } from './conversations-api.service';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

@Component({
  selector: 'app-conversation-summary',
  standalone: true,
  providers: [provideTranslocoScope('conversations')],
  imports: [ButtonComponent, TranslocoPipe, ErrorReferenceComponent],
  templateUrl: './conversation-summary.component.html',
  styles: [
    `
      .summary-panel {
        padding: var(--app-space-3) var(--app-space-4);
        margin: 0 var(--app-space-4) var(--app-space-3);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        border: 1px solid var(--app-border);
      }
      .summary-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-2);
      }
      .summary-label {
        font-size: var(--app-font-sm);
        font-weight: 700;
        color: var(--app-text);
        text-transform: uppercase;
        letter-spacing: 0.05em;
      }
      .generating {
        font-size: var(--app-font-sm);
        color: var(--app-text-2);
        font-weight: 600;
      }
      .summary-text {
        margin: var(--app-space-2) 0 0;
        font-size: var(--app-font-sm);
        color: var(--app-text);
        line-height: 1.5;
        white-space: pre-wrap;
      }
      .summary-error {
        margin: var(--app-space-2) 0 0;
        font-size: var(--app-font-sm);
        color: var(--app-red);
        font-weight: 600;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ConversationSummaryComponent {
  readonly conversationId = input.required<string>();

  private readonly api = inject(ConversationsApiService);

  protected readonly summary = signal<string | null>(null);
  protected readonly loading = signal(false);
  /** What went wrong, kept whole so the screen can show its text and its request reference. */
  protected readonly error = signal<ApiError | null>(null);

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  protected generate(): void {
    this.loading.set(true);
    this.error.set(null);
    this.api
      .requestSummary(this.conversationId())
      .pipe(
        catchError((err) => {
          this.error.set(toApiError(err));
          return of(null);
        }),
        finalize(() => this.loading.set(false)),
      )
      .subscribe((result) => {
        if (result) {
          this.summary.set(result.data.summary);
        }
      });
  }
}
