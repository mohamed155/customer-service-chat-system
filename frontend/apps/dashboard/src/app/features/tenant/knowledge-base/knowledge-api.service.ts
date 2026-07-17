import { HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';
import { APP_CONFIG } from '../../../core/config/app-config';
import { ApiService } from '../../../core/api/api.service';
import { ApiResponse } from '../../../core/api/api.models';
import {
  CreateCategoryPayload,
  CreateItemPayload,
  ItemFilters,
  ItemListResponse,
  KnowledgeCategory,
  KnowledgeItemDetail,
  RenameCategoryPayload,
  ReindexResponse,
  SetStatusPayload,
  SetStatusResponse,
  UpdateItemPayload,
} from '../../../core/api/knowledge.models';

@Injectable({ providedIn: 'root' })
export class KnowledgeApiService {
  private readonly api = inject(ApiService);
  private readonly config = inject(APP_CONFIG);

  listItems(filters?: ItemFilters, cursor?: string): Observable<ApiResponse<ItemListResponse>> {
    let params = new HttpParams();
    if (filters?.type) params = params.set('type', filters.type);
    if (filters?.status) params = params.set('status', filters.status);
    if (filters?.categoryId) params = params.set('categoryId', filters.categoryId);
    if (filters?.tag) params = params.set('tag', filters.tag);
    if (filters?.q) params = params.set('q', filters.q);
    if (cursor) params = params.set('before', cursor);
    return this.api.get('tenant/knowledge/items', params);
  }

  getItem(id: string): Observable<ApiResponse<KnowledgeItemDetail>> {
    return this.api.get(`tenant/knowledge/items/${id}`);
  }

  createItem(payload: CreateItemPayload): Observable<ApiResponse<KnowledgeItemDetail>> {
    return this.api.post('tenant/knowledge/items', payload);
  }

  updateItem(id: string, payload: UpdateItemPayload): Observable<ApiResponse<KnowledgeItemDetail>> {
    return this.api.patch(`tenant/knowledge/items/${id}`, payload);
  }

  setStatus(id: string, payload: SetStatusPayload): Observable<ApiResponse<SetStatusResponse>> {
    return this.api.post(`tenant/knowledge/items/${id}/status`, payload);
  }

  listCategories(): Observable<ApiResponse<KnowledgeCategory[]>> {
    return this.api.get('tenant/knowledge/categories');
  }

  createCategory(payload: CreateCategoryPayload): Observable<ApiResponse<KnowledgeCategory>> {
    return this.api.post('tenant/knowledge/categories', payload);
  }

  renameCategory(
    id: string,
    payload: RenameCategoryPayload,
  ): Observable<ApiResponse<KnowledgeCategory>> {
    return this.api.patch(`tenant/knowledge/categories/${id}`, payload);
  }

  deleteCategory(id: string): Observable<ApiResponse<void>> {
    return this.api.delete(`tenant/knowledge/categories/${id}`);
  }

  uploadDocument(formData: FormData): Observable<ApiResponse<KnowledgeItemDetail>> {
    return this.api.post('tenant/knowledge/documents', formData);
  }

  reindex(id: string): Observable<ApiResponse<ReindexResponse>> {
    return this.api.post(`tenant/knowledge/items/${id}/reindex`, {});
  }

  fileDownloadUrl(id: string): string {
    const base = this.config.apiBaseUrl.replace(/\/$/, '');
    return `${base}/tenant/knowledge/items/${id}/file`;
  }
}
