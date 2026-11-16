import { HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  Conversation,
  conversationFromWire,
  ConversationWire,
} from '../../../core/api/tenant-api.models';

/**
 * What the overview reads that no other page shares: the latest conversations. The analytics
 * summary and trend come from `AnalyticsApiService`, which the analytics page uses too.
 */
@Injectable({ providedIn: 'root' })
export class OverviewApiService {
  private readonly api = inject(ApiService);

  /** The newest conversations, most recently active first (the API's own order). */
  recentConversations(limit: number): Observable<ApiResponse<Conversation[]>> {
    const params = new HttpParams().set('limit', String(limit));
    return this.api
      .get<{ data: ConversationWire[] }>('/tenant/conversations', params)
      .pipe(map(({ data, ...rest }) => ({ ...rest, data: data.data.map(conversationFromWire) })));
  }
}
