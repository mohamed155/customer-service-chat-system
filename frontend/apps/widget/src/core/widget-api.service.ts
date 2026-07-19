import { Injectable, InjectionToken, inject } from '@angular/core';
import { HttpClient, HttpErrorResponse, HttpHeaders } from '@angular/common/http';
import { Observable, throwError } from 'rxjs';
import { catchError, map } from 'rxjs/operators';
import {
  WidgetConfig,
  SessionResponse,
  WidgetConversation,
  WidgetMessage,
  WidgetFeedback,
  PendingFeedback,
  RateLimitedError,
  SessionExpiredError,
} from './models';

export const WIDGET_API_BASE = new InjectionToken<string>('WIDGET_API_BASE');

@Injectable({ providedIn: 'root' })
export class WidgetApiService {
  private readonly http = inject(HttpClient);
  private readonly base = inject(WIDGET_API_BASE);

  private headers(token?: string): HttpHeaders {
    let h = new HttpHeaders({ 'Content-Type': 'application/json' });
    if (token) h = h.set('Authorization', `Bearer ${token}`);
    return h;
  }

  private mapError(err: HttpErrorResponse): never {
    if (err.status === 429) throw new RateLimitedError();
    if (err.status === 401) throw new SessionExpiredError();
    throw err;
  }

  getConfig(widgetId: string): Observable<WidgetConfig> {
    return this.http
      .get<{ data: WidgetConfig }>(`${this.base}/widget/v1/config`, {
        params: { widgetId },
      })
      .pipe(map((r) => r.data));
  }

  createSession(widgetId: string): Observable<SessionResponse> {
    return this.http
      .post<{ data: SessionResponse }>(`${this.base}/widget/v1/sessions`, {
        widgetId,
      })
      .pipe(
        map((r) => r.data),
        catchError((err: HttpErrorResponse) => throwError(() => this.mapError(err))),
      );
  }

  getConversation(
    token: string,
  ): Observable<{ data: { conversation: WidgetConversation } | null }> {
    return this.http
      .get<{ data: { conversation: WidgetConversation } | null }>(
        `${this.base}/widget/v1/conversation`,
        { headers: this.headers(token) },
      )
      .pipe(catchError((err: HttpErrorResponse) => throwError(() => this.mapError(err))));
  }

  createConversation(token: string): Observable<WidgetConversation> {
    return this.http
      .post<{ data: { conversation: WidgetConversation } }>(
        `${this.base}/widget/v1/conversations`,
        {},
        { headers: this.headers(token) },
      )
      .pipe(
        map((r) => r.data.conversation),
        catchError((err: HttpErrorResponse) => throwError(() => this.mapError(err))),
      );
  }

  sendMessage(token: string, conversationId: string, body: string): Observable<WidgetMessage> {
    return this.http
      .post<{ data: { message: WidgetMessage } }>(
        `${this.base}/widget/v1/conversations/${conversationId}/messages`,
        { body },
        { headers: this.headers(token) },
      )
      .pipe(
        map((r) => r.data.message),
        catchError((err: HttpErrorResponse) => throwError(() => this.mapError(err))),
      );
  }

  getPendingFeedback(token: string): Observable<PendingFeedback | null> {
    return this.http
      .get<{ data: PendingFeedback | null }>(`${this.base}/widget/v1/feedback/pending`, {
        headers: this.headers(token),
      })
      .pipe(
        map((r) => r.data),
        catchError((err: HttpErrorResponse) => throwError(() => this.mapError(err))),
      );
  }

  submitFeedback(
    token: string,
    conversationId: string,
    rating: number,
    comment?: string | null,
  ): Observable<WidgetFeedback> {
    return this.http
      .post<{ data: { feedback: WidgetFeedback } }>(
        `${this.base}/widget/v1/conversations/${conversationId}/feedback`,
        { rating, comment: comment ?? null },
        { headers: this.headers(token) },
      )
      .pipe(
        map((r) => r.data.feedback),
        catchError((err: HttpErrorResponse) => throwError(() => this.mapError(err))),
      );
  }

  eventsUrl(token: string, conversationId: string): string {
    return `${this.base}/widget/v1/conversations/${conversationId}/events`;
  }
}
