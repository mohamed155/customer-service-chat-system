import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { catchError, defer, finalize, map, Observable, of, shareReplay, throwError } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { toApiError } from '../errors/api-error';

/**
 * Exchanges the refresh cookie for a new access token (`POST /auth/refresh`, ADR-005 §3).
 *
 * The access token lives ten minutes. When a call answers `401`, the interceptor asks here for a
 * new one and sends the call again. The refresh token rotates on every use, so two refreshes at
 * once would spend it twice: every caller that arrives while one is in flight waits for that one
 * and gets its answer (single flight).
 */
@Injectable({ providedIn: 'root' })
export class SessionRefreshService {
  private readonly http = inject(HttpClient);
  private readonly config = inject(APP_CONFIG);
  private inFlight: Observable<boolean> | null = null;

  /**
   * `true` when the session was refreshed and the new cookies are set; `false` when the refresh
   * token was refused (`401`: unknown, expired, used again, or the session is over), which is the
   * one answer that means "sign in again". Any other failure (a `429`, a `503`, a timeout, a
   * dropped connection) is thrown as it is: the refresh token may still be good, so it must not be
   * taken for an expired session.
   */
  refresh(): Observable<boolean> {
    this.inFlight ??= defer(() =>
      this.http.post<void>(`${this.config.apiBaseUrl.replace(/\/$/, '')}/auth/refresh`, null),
    ).pipe(
      map(() => true),
      catchError((error: unknown) =>
        toApiError(error).status === 401 ? of(false) : throwError(() => error),
      ),
      finalize(() => {
        this.inFlight = null;
      }),
      // Late callers share the answer; the stream completes with the first.
      shareReplay({ bufferSize: 1, refCount: false }),
    );
    return this.inFlight;
  }
}
