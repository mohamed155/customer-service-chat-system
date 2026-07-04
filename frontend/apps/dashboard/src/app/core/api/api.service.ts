import { HttpClient, HttpParams, HttpResponse } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { ApiListQuery, ApiResponse, PaginatedResponse } from './api.models';

@Injectable({ providedIn: 'root' })
export class ApiService {
  private readonly http = inject(HttpClient);
  private readonly config = inject(APP_CONFIG);
  get<T>(path: string, params?: HttpParams): Observable<ApiResponse<T>> {
    return this.wrap(this.http.get<T>(this.url(path), { params, observe: 'response' }));
  }
  post<T>(path: string, body: unknown): Observable<ApiResponse<T>> {
    return this.wrap(this.http.post<T>(this.url(path), body, { observe: 'response' }));
  }
  patch<T>(path: string, body: unknown): Observable<ApiResponse<T>> {
    return this.wrap(this.http.patch<T>(this.url(path), body, { observe: 'response' }));
  }
  delete<T>(path: string): Observable<ApiResponse<T>> {
    return this.wrap(this.http.delete<T>(this.url(path), { observe: 'response' }));
  }
  list<T>(path: string, query: ApiListQuery = {}): Observable<ApiResponse<PaginatedResponse<T>>> {
    let params = new HttpParams();
    for (const [key, value] of Object.entries(query))
      if (value !== undefined) params = params.set(key, String(value));
    return this.get<PaginatedResponse<T>>(path, params);
  }
  private url(path: string): string {
    return `${this.config.apiBaseUrl.replace(/\/$/, '')}/${path.replace(/^\//, '')}`;
  }
  private wrap<T>(request: Observable<HttpResponse<T>>): Observable<ApiResponse<T>> {
    return request.pipe(
      map((response) => ({
        data: response.body as T,
        ...(response.headers.get('X-Request-Id')
          ? { requestId: response.headers.get('X-Request-Id') as string }
          : {}),
      })),
    );
  }
}
