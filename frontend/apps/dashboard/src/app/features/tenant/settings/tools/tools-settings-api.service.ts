import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiService } from '../../../../core/api/api.service';
import {
  CreateTenantToolPayload,
  TenantDefinedTool,
  ToolsSettingsResponse,
  UpdateTenantToolPayload,
} from '../../../../core/api/tenant-api.models';

@Injectable({ providedIn: 'root' })
export class ToolsSettingsApiService {
  private readonly api = inject(ApiService);

  getTools(): Observable<ToolsSettingsResponse> {
    return this.api.get<ToolsSettingsResponse>('/tenant/tools').pipe(map(({ data }) => data));
  }

  updateBuiltinPolicy(name: string, enabled: boolean, requireApproval: boolean): Observable<void> {
    return this.api
      .put<void>(`/tenant/tools/builtin/${name}/policy`, {
        enabled,
        require_approval: requireApproval,
      })
      .pipe(map(() => undefined));
  }

  createTenantTool(payload: CreateTenantToolPayload): Observable<TenantDefinedTool> {
    const wire = {
      name: payload.name,
      description: payload.description,
      input_schema: payload.inputSchema,
      endpoint_url: payload.endpointUrl,
      credential: payload.credential,
      classification: payload.classification,
      enabled: payload.enabled,
    };
    return this.api.post<TenantDefinedTool>('/tenant/tools', wire).pipe(map(({ data }) => data));
  }

  updateTenantTool(id: string, payload: UpdateTenantToolPayload): Observable<TenantDefinedTool> {
    const wire: Record<string, unknown> = {};
    if ('name' in payload) wire['name'] = payload.name;
    if ('description' in payload) wire['description'] = payload.description;
    if ('inputSchema' in payload) wire['input_schema'] = payload.inputSchema;
    if ('endpointUrl' in payload) wire['endpoint_url'] = payload.endpointUrl;
    if ('credential' in payload) wire['credential'] = payload.credential;
    if ('classification' in payload) wire['classification'] = payload.classification;
    if ('enabled' in payload) wire['enabled'] = payload.enabled;
    return this.api
      .put<TenantDefinedTool>(`/tenant/tools/${id}`, wire)
      .pipe(map(({ data }) => data));
  }

  deleteTenantTool(id: string): Observable<void> {
    return this.api.delete<void>(`/tenant/tools/${id}`).pipe(map(() => undefined));
  }
}
