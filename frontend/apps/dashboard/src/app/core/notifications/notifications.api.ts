import { Injectable, inject } from '@angular/core';
import { HttpParams } from '@angular/common/http';
import { map, Observable } from 'rxjs';
import { ApiService } from '../api/api.service';
import { ApiResponse } from '../api/api.models';
import type { components } from '../api/generated/api';
import {
  NotificationEntry,
  notificationFromWire,
  notificationListFromWire,
} from '../api/tenant-api.models';

type Schemas = components['schemas'];

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
    return this.api.get<Schemas['NotificationListResponse']>('/tenant/notifications', params).pipe(
      map(({ data, ...rest }) => ({
        ...rest,
        data: notificationListFromWire(data),
      })),
    );
  }

  unreadCount(): Observable<ApiResponse<{ count: number }>> {
    return this.api.get<Schemas['UnreadCountResponse']>('/tenant/notifications/unread-count');
  }

  markRead(id: string): Observable<ApiResponse<NotificationEntry>> {
    return this.api
      .post<Schemas['NotificationDto']>(`/tenant/notifications/${id}/read`, null)
      .pipe(map(({ data, ...rest }) => ({ ...rest, data: notificationFromWire(data) })));
  }

  markAllRead(): Observable<ApiResponse<{ marked: number }>> {
    return this.api.post<Schemas['MarkedResponse']>('/tenant/notifications/read-all', null);
  }
}
