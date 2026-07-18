import { HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  AddMessagePayload,
  addMessagePayloadToWire,
  Conversation,
  ConversationDetail,
  ConversationDetailEscalation,
  conversationDetailEscalationFromWire,
  conversationDetailFromWire,
  ConversationDetailEscalationWire,
  ConversationDetailWire,
  ConversationListQuery,
  conversationFromWire,
  ConversationWire,
  CreateConversationPayload,
  createConversationPayloadToWire,
  DecideToolRequestRequest,
  Message,
  messageFromWire,
  MessageWire,
  PatchConversationPayload,
  patchPayloadToWire,
  TeamMember,
  ToolRequest,
} from '../../../core/api/tenant-api.models';

interface ConversationListWireResponse {
  readonly data: ConversationWire[];
  readonly pagination: {
    readonly next_cursor: string | null;
    readonly has_more: boolean;
  };
}

@Injectable({ providedIn: 'root' })
export class ConversationsApiService {
  private readonly api = inject(ApiService);

  list(
    query: ConversationListQuery = {},
  ): Observable<ApiResponse<PaginatedResponse<Conversation>>> {
    return this.api
      .get<ConversationListWireResponse>('/tenant/conversations', this.buildParams(query))
      .pipe(
        map(({ data, ...response }) => ({
          ...response,
          data: {
            items: data.data.map(conversationFromWire),
            nextCursor: data.pagination.next_cursor,
            hasMore: data.pagination.has_more,
          },
        })),
      );
  }

  listAssignableMembers(): Observable<ApiResponse<TeamMember[]>> {
    return this.api.get<TeamMember[]>('/tenant/members');
  }

  getToolActivity(conversationId: string): Observable<ApiResponse<{ items: ToolRequest[] }>> {
    return this.api.get<{ items: ToolRequest[] }>(
      `/tenant/conversations/${conversationId}/tool-activity`,
    );
  }

  decideToolRequest(
    id: string,
    decision: 'approve' | 'deny',
  ): Observable<ApiResponse<ToolRequest>> {
    const payload: DecideToolRequestRequest = { decision };
    return this.api.post<ToolRequest>(`/tenant/tool-requests/${id}/decide`, payload);
  }

  get(id: string): Observable<ApiResponse<ConversationDetailEscalation>> {
    return this.api
      .get<{ data: ConversationDetailEscalationWire }>(`/tenant/conversations/${id}`)
      .pipe(
        map(({ data, ...response }) => ({
          ...response,
          data: conversationDetailEscalationFromWire(data.data),
        })),
      );
  }

  getTimeline(id: string, cursor?: string): Observable<ApiResponse<PaginatedResponse<Message>>> {
    const params = cursor ? new HttpParams().set('cursor', cursor) : undefined;
    return this.api
      .get<{
        data: MessageWire[];
        pagination: { next_cursor: string | null; has_more: boolean };
      }>(`/tenant/conversations/${id}/messages`, params)
      .pipe(
        map(({ data, ...response }) => ({
          ...response,
          data: {
            items: data.data.map(messageFromWire),
            nextCursor: data.pagination.next_cursor,
            hasMore: data.pagination.has_more,
          },
        })),
      );
  }

  addMessage(
    conversationId: string,
    payload: AddMessagePayload,
  ): Observable<
    ApiResponse<{
      message: Message;
      conversation: { status: Conversation['status']; lastActivityAt: string };
    }>
  > {
    const wirePayload = addMessagePayloadToWire(payload);
    return this.api
      .post<{
        data: {
          message: MessageWire;
          conversation: { status: Conversation['status']; last_activity_at: string };
        };
      }>(`/tenant/conversations/${conversationId}/messages`, wirePayload)
      .pipe(
        map(({ data, ...response }) => ({
          ...response,
          data: {
            message: messageFromWire(data.data.message),
            conversation: {
              status: data.data.conversation.status,
              lastActivityAt: data.data.conversation.last_activity_at,
            },
          },
        })),
      );
  }

  patch(
    id: string,
    payload: PatchConversationPayload,
  ): Observable<ApiResponse<ConversationDetail>> {
    const wirePayload = patchPayloadToWire(payload);
    return this.api
      .patch<{ data: ConversationDetailWire }>(`/tenant/conversations/${id}`, wirePayload)
      .pipe(
        map(({ data, ...response }) => ({
          ...response,
          data: conversationDetailFromWire(data.data),
        })),
      );
  }

  setConversationAiHandling(
    conversationId: string,
    mode: 'platform_ai' | 'human',
  ): Observable<ApiResponse<ConversationDetail>> {
    return this.api
      .post<{ data: ConversationDetailWire }>(
        `tenant/conversations/${conversationId}/ai-handling`,
        { mode },
      )
      .pipe(
        map(({ data, ...response }) => ({
          ...response,
          data: conversationDetailFromWire(data.data),
        })),
      );
  }

  requestSummary(conversationId: string): Observable<
    ApiResponse<{
      summary: string;
      generatedAt: string;
      messageCount: number;
    }>
  > {
    return this.api
      .post<{
        data: {
          summary: string;
          generated_at: string;
          message_count: number;
        };
      }>(`/tenant/conversations/${conversationId}/summary`, undefined)
      .pipe(
        map(({ data, ...response }) => ({
          ...response,
          data: {
            summary: data.data.summary,
            generatedAt: data.data.generated_at,
            messageCount: data.data.message_count,
          },
        })),
      );
  }

  create(payload: CreateConversationPayload): Observable<ApiResponse<ConversationDetail>> {
    const wirePayload = createConversationPayloadToWire(payload);
    return this.api
      .post<{ data: ConversationDetailWire }>('/tenant/conversations', wirePayload)
      .pipe(
        map(({ data, ...response }) => ({
          ...response,
          data: conversationDetailFromWire(data.data),
        })),
      );
  }

  private buildParams(query: ConversationListQuery): HttpParams {
    let params = new HttpParams();
    for (const [key, value] of Object.entries(query))
      if (value !== undefined && value !== null) params = params.set(key, String(value));
    return params;
  }
}
