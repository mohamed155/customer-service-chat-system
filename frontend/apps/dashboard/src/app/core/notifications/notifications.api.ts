import { Injectable, inject } from '@angular/core';
import { HttpParams } from '@angular/common/http';
import { map, Observable } from 'rxjs';
import { ApiService } from '../api/api.service';
import { ApiResponse } from '../api/api.models';
import {
  NotificationEntry,
  NotificationListWire,
  NotificationWire,
  notificationFromWire,
  notificationListFromWire,
} from '../api/tenant-api.models';

@Injectable({ providedIn: 'root' })
export class NotificationsApiService {
  private readonly api = inject(ApiService);

  list(
    state?: string,
    cursor?: string,
  ): Observable<
    ApiResponse<{ items: NotificationEntry[]; hasMore: boolean; nextCursor: string | null }>
  > {
    let params = new HttpParams();
    if (state) params = params.set('state', state);
    if (cursor) params = params.set('cursor', cursor);
    return this.api.get<NotificationListWire>('/tenant/notifications', params).pipe(
      map(({ data, ...rest }) => ({
        ...rest,
        data: notificationListFromWire(data),
      })),
    );
  }

  unreadCount(): Observable<ApiResponse<{ count: number }>> {
    return this.api.get<{ count: number }>('/tenant/notifications/unread-count');
  }

  markRead(id: string): Observable<ApiResponse<NotificationEntry>> {
    return this.api
      .post<NotificationWire>(`/tenant/notifications/${id}/read`, null)
      .pipe(map(({ data, ...rest }) => ({ ...rest, data: notificationFromWire(data) })));
  }

  markAllRead(): Observable<ApiResponse<{ marked: number }>> {
    return this.api.post<{ marked: number }>('/tenant/notifications/read-all', null);
  }
}
