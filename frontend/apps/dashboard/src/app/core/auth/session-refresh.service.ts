import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { catchError, defer, finalize, map, Observable, of, shareReplay } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';

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

  /** `true` when the session was refreshed and the new cookies are set; `false` when it was not. */
  refresh(): Observable<boolean> {
    this.inFlight ??= defer(() =>
      this.http.post<void>(`${this.config.apiBaseUrl.replace(/\/$/, '')}/auth/refresh`, null),
    ).pipe(
      map(() => true),
      // Any failure means the same thing to the caller: the session is over, sign in again.
      catchError(() => of(false)),
      finalize(() => {
        this.inFlight = null;
      }),
      // Late callers share the answer; the stream completes with the first.
      shareReplay({ bufferSize: 1, refCount: false }),
    );
    return this.inFlight;
  }
}
