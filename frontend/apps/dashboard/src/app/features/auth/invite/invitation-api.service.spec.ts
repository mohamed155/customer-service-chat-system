import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of } from 'rxjs';
import { ApiService } from '../../../core/api/api.service';
import { AcceptInvitationRequest, InvitationPreview } from '../../../core/api/tenant-api.models';
import { InvitationApiService } from './invitation-api.service';

describe('InvitationApiService', () => {
  const api = { get: vi.fn(), post: vi.fn() };
  let service: InvitationApiService;

  const mockPreview: InvitationPreview = {
    tenantName: 'Acme Corp',
    email: 'user@acme.com',
    role: 'agent',
    expiresAt: '2026-02-01T00:00:00Z',
    accountExists: false,
  };

  beforeEach(() => {
    api.get.mockReset();
    api.post.mockReset();
    TestBed.configureTestingModule({
      providers: [InvitationApiService, { provide: ApiService, useValue: api }],
    });
    service = TestBed.inject(InvitationApiService);
  });

  describe('previewInvitation', () => {
    it('calls api.get with the invitation token', async () => {
      api.get.mockReturnValue(of({ data: mockPreview }));

      const result = await firstValueFrom(service.previewInvitation('token-123'));

      expect(api.get).toHaveBeenCalledWith('invitations/token-123');
      expect(result.data).toEqual(mockPreview);
    });
  });

  describe('acceptInvitation', () => {
    it('calls api.post with the token and payload', async () => {
      const payload: AcceptInvitationRequest = { displayName: 'Alice', password: 'secret123' };
      api.post.mockReturnValue(of({ data: { id: 'u-1' } }));

      await firstValueFrom(service.acceptInvitation('token-123', payload));

      expect(api.post).toHaveBeenCalledWith('invitations/token-123/accept', payload);
    });
  });
});
