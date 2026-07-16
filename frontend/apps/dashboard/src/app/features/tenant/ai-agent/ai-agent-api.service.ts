import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';
import { map } from 'rxjs/operators';
import { ApiService } from '../../../core/api/api.service';
import { ApiResponse } from '../../../core/api/api.models';
import {
  AgentConfigPayload,
  AgentConfigResponse,
  AgentOptionsResponse,
  AvatarUpdateResponse,
} from '../../../core/api/ai-agent.models';
import { APP_CONFIG } from '../../../core/config/app-config';

@Injectable({ providedIn: 'root' })
export class AiAgentApiService {
  private readonly api = inject(ApiService);
  private readonly http = inject(HttpClient);
  private readonly config = inject(APP_CONFIG);

  getAgent(): Observable<ApiResponse<AgentConfigResponse>> {
    return this.api.get<AgentConfigResponse>('tenant/ai/agent');
  }

  saveAgent(payload: AgentConfigPayload): Observable<ApiResponse<AgentConfigResponse>> {
    return this.api.put<AgentConfigResponse>('tenant/ai/agent', payload);
  }

  getOptions(): Observable<ApiResponse<AgentOptionsResponse>> {
    return this.api.get<AgentOptionsResponse>('tenant/ai/agent/options');
  }

  uploadAvatar(blob: Blob, contentType: string): Observable<ApiResponse<AvatarUpdateResponse>> {
    return this.http
      .put<AvatarUpdateResponse>(
        `${this.config.apiBaseUrl.replace(/\/$/, '')}/tenant/ai/agent/avatar`,
        blob,
        { headers: { 'Content-Type': contentType }, observe: 'response' },
      )
      .pipe(
        map((response) => ({
          data: response.body as AvatarUpdateResponse,
          ...(response.headers.get('X-Request-Id')
            ? { requestId: response.headers.get('X-Request-Id') as string }
            : {}),
        })),
      );
  }

  getAvatarUrl(): string {
    return `${this.config.apiBaseUrl.replace(/\/$/, '')}/tenant/ai/agent/avatar`;
  }

  setConversationAiHandling(
    conversationId: string,
    mode: 'platform_ai' | 'human',
  ): Observable<ApiResponse<unknown>> {
    return this.api.post<unknown>(`tenant/conversations/${conversationId}/ai-handling`, { mode });
  }
}
