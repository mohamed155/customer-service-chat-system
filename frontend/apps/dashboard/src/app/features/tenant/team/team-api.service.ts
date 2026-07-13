import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';
import { ApiService } from '../../../core/api/api.service';
import { ApiListQuery, ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import {
  TeamMember,
  TeamMemberQuery,
  InvitationQuery,
  TenantInvitation,
  CreateInvitationPayload,
  CreateInvitationResponse,
  InvitationPreview,
  AcceptInvitationRequest,
  PatchMemberPayload,
  MeResponse,
  InvitationDeliveryResponse,
} from '../../../core/api/tenant-api.models';

@Injectable({ providedIn: 'root' })
export class TeamApiService {
  private readonly api = inject(ApiService);

  getMembers(query: TeamMemberQuery = {}): Observable<ApiResponse<PaginatedResponse<TeamMember>>> {
    return this.api.list<TeamMember>('tenant/members', query as ApiListQuery);
  }

  patchMember(id: string, payload: PatchMemberPayload): Observable<ApiResponse<TeamMember>> {
    return this.api.patch<TeamMember>(`tenant/members/${id}`, payload);
  }

  getInvitations(
    query: InvitationQuery = {},
  ): Observable<ApiResponse<PaginatedResponse<TenantInvitation>>> {
    return this.api.list<TenantInvitation>('tenant/members/invitations', query);
  }

  createInvitation(
    payload: CreateInvitationPayload,
  ): Observable<ApiResponse<CreateInvitationResponse>> {
    return this.api.post<CreateInvitationResponse>('tenant/members/invitations', payload);
  }

  getInvitationDelivery(id: string): Observable<ApiResponse<InvitationDeliveryResponse>> {
    return this.api.get<InvitationDeliveryResponse>(`tenant/members/invitations/${id}/delivery`);
  }

  revokeInvitation(id: string): Observable<ApiResponse<void>> {
    return this.api.delete<void>(`tenant/members/invitations/${id}`);
  }

  previewInvitation(token: string): Observable<ApiResponse<InvitationPreview>> {
    return this.api.get<InvitationPreview>(`invitations/${token}`);
  }

  acceptInvitation(
    token: string,
    payload: AcceptInvitationRequest,
  ): Observable<ApiResponse<MeResponse>> {
    return this.api.post<MeResponse>(`invitations/${token}/accept`, payload);
  }
}
