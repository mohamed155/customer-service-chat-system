import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  AcceptInvitationRequest,
  InvitationPreview,
  MeResponse,
} from '../../../core/api/tenant-api.models';

/** The two calls an invited person makes before they have a session: look at, then accept. */
@Injectable({ providedIn: 'root' })
export class InvitationApiService {
  private readonly api = inject(ApiService);

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
