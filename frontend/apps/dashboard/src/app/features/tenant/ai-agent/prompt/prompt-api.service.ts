import { HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';
import { ApiService } from '../../../../core/api/api.service';
import { ApiResponse } from '../../../../core/api/api.models';
import {
  PromptBootstrapResponse,
  PromptSavePayload,
  PromptSaveResponse,
  PromptVersionDetail,
  PromptVersionListResponse,
  RestorePayload,
} from '../../../../core/api/ai-agent.models';

@Injectable({ providedIn: 'root' })
export class PromptApiService {
  private readonly api = inject(ApiService);

  getPrompt(): Observable<ApiResponse<PromptBootstrapResponse>> {
    return this.api.get<PromptBootstrapResponse>('tenant/ai/agent/prompt');
  }

  savePrompt(payload: PromptSavePayload): Observable<ApiResponse<PromptSaveResponse>> {
    return this.api.put<PromptSaveResponse>('tenant/ai/agent/prompt', payload);
  }

  listVersions(
    limit?: number,
    before?: number,
  ): Observable<ApiResponse<PromptVersionListResponse>> {
    let params = new HttpParams();
    if (limit !== undefined) params = params.set('limit', String(limit));
    if (before !== undefined) params = params.set('before', String(before));
    return this.api.get<PromptVersionListResponse>('tenant/ai/agent/prompt/versions', params);
  }

  getVersion(versionNumber: number): Observable<ApiResponse<PromptVersionDetail>> {
    return this.api.get<PromptVersionDetail>(`tenant/ai/agent/prompt/versions/${versionNumber}`);
  }

  restoreVersion(
    versionNumber: number,
    baseVersion: number,
  ): Observable<ApiResponse<PromptSaveResponse>> {
    return this.api.post<PromptSaveResponse>(
      `tenant/ai/agent/prompt/versions/${versionNumber}/restore`,
      { baseVersion } as RestorePayload,
    );
  }
}
