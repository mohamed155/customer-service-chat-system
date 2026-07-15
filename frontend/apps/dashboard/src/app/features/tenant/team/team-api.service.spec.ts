import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of, throwError } from 'rxjs';
import { ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  AcceptInvitationRequest,
  CreateInvitationPayload,
  CreateInvitationResponse,
  InvitationPreview,
  PatchMemberPayload,
  TeamMember,
  TenantInvitation,
  Skill,
} from '../../../core/api/tenant-api.models';
import { TeamApiService } from './team-api.service';

describe('TeamApiService', () => {
  let service: TeamApiService;
  let api: {
    list: ReturnType<typeof vi.fn>;
    get: ReturnType<typeof vi.fn>;
    post: ReturnType<typeof vi.fn>;
    patch: ReturnType<typeof vi.fn>;
    delete: ReturnType<typeof vi.fn>;
    put: ReturnType<typeof vi.fn>;
  };

  const mockInvitation: TenantInvitation = {
    id: 'i-1',
    email: 'user@test.com',
    role: 'agent',
    status: 'pending',
    invitedByName: 'Admin',
    emailDeliveryStatus: 'unconfigured',
    createdAt: '2026-01-01T00:00:00Z',
    expiresAt: '2026-02-01T00:00:00Z',
  };

  const mockPreview: InvitationPreview = {
    tenantName: 'Acme Corp',
    email: 'user@acme.com',
    role: 'agent',
    expiresAt: '2026-02-01T00:00:00Z',
    accountExists: false,
  };

  beforeEach(() => {
    api = {
      list: vi.fn(),
      get: vi.fn(),
      post: vi.fn(),
      patch: vi.fn(),
      delete: vi.fn(),
      put: vi.fn(),
    };
    TestBed.configureTestingModule({
      providers: [TeamApiService, { provide: ApiService, useValue: api }],
    });
    service = TestBed.inject(TeamApiService);
  });

  describe('getMembers', () => {
    it('calls api.list with tenant/members and the query', async () => {
      const response: ApiResponse<PaginatedResponse<TeamMember>> = {
        data: { items: [], nextCursor: null, hasMore: false },
      };
      api.list.mockReturnValue(of(response));

      const result = await firstValueFrom(service.getMembers({ limit: 25, status: 'active' }));

      expect(api.list).toHaveBeenCalledWith('tenant/members', { limit: 25, status: 'active' });
      expect(result).toEqual(response);
    });

    it('calls api.list with default empty query', async () => {
      api.list.mockReturnValue(of({ data: { items: [], nextCursor: null, hasMore: false } }));

      await firstValueFrom(service.getMembers());

      expect(api.list).toHaveBeenCalledWith('tenant/members', {});
    });

    it('propagates errors from the underlying ApiService', async () => {
      const error = { code: 'forbidden', message: 'Access denied', status: 403 };
      api.list.mockReturnValue(throwError(() => error));

      await expect(firstValueFrom(service.getMembers())).rejects.toEqual(error);
    });
  });

  describe('patchMember', () => {
    it('calls api.patch with the member id and payload', async () => {
      const payload: PatchMemberPayload = { role: 'admin' };
      const member: TeamMember = {
        id: 'm-1',
        userId: 'u-1',
        displayName: 'Alice',
        email: 'alice@test.com',
        role: 'admin',
        status: 'active',
        joinedAt: '2026-01-01T00:00:00Z',
      };
      api.patch.mockReturnValue(of({ data: member }));

      const result = await firstValueFrom(service.patchMember('m-1', payload));

      expect(api.patch).toHaveBeenCalledWith('tenant/members/m-1', payload);
      expect(result.data).toEqual(member);
    });
  });

  describe('getInvitations', () => {
    it('calls api.list with tenant/members/invitations', async () => {
      const response: ApiResponse<PaginatedResponse<TenantInvitation>> = {
        data: { items: [mockInvitation], nextCursor: null, hasMore: false },
      };
      api.list.mockReturnValue(of(response));

      const result = await firstValueFrom(service.getInvitations());

      expect(api.list).toHaveBeenCalledWith('tenant/members/invitations', {});
      expect(result).toEqual(response);
    });
  });

  describe('createInvitation', () => {
    it('calls api.post with the invitation payload', async () => {
      const payload: CreateInvitationPayload = { email: 'new@test.com', role: 'agent' };
      const responseData: CreateInvitationResponse = {
        invitation: mockInvitation,
        acceptUrl: 'https://example.com/invite/token',
        emailSent: true,
        emailDeliveryStatus: 'sent',
      };
      api.post.mockReturnValue(of({ data: responseData }));

      const result = await firstValueFrom(service.createInvitation(payload));

      expect(api.post).toHaveBeenCalledWith('tenant/members/invitations', payload);
      expect(result.data).toEqual(responseData);
    });
  });

  describe('getInvitationDelivery', () => {
    it('calls the targeted tenant invitation delivery endpoint', async () => {
      api.get.mockReturnValue(of({ data: { emailDeliveryStatus: 'queued' } }));

      const result = await firstValueFrom(service.getInvitationDelivery('i-1'));

      expect(api.get).toHaveBeenCalledWith('tenant/members/invitations/i-1/delivery');
      expect(result.data.emailDeliveryStatus).toBe('queued');
    });
  });

  describe('revokeInvitation', () => {
    it('calls api.delete with the invitation id', async () => {
      api.delete.mockReturnValue(of({ data: undefined }));

      const result = await firstValueFrom(service.revokeInvitation('i-1'));

      expect(api.delete).toHaveBeenCalledWith('tenant/members/invitations/i-1');
      expect(result.data).toBeUndefined();
    });
  });

  describe('previewInvitation', () => {
    it('calls api.get with the invitation token', async () => {
      api.get.mockReturnValue(of({ data: mockPreview }));

      const result = await firstValueFrom(service.previewInvitation('token-123'));

      expect(api.get).toHaveBeenCalledWith('invitations/token-123');
      expect(result.data).toEqual(mockPreview);
    });
  });

  describe('getSkills', () => {
    it('calls api.get with tenant/skills', async () => {
      const skills: Skill[] = [
        { id: 's-1', name: 'billing', agentCount: 3 },
        { id: 's-2', name: 'support', agentCount: 5 },
      ];
      api.get.mockReturnValue(of({ data: skills }));

      const result = await firstValueFrom(service.getSkills());

      expect(api.get).toHaveBeenCalledWith('tenant/skills');
      expect(result.data).toEqual(skills);
    });
  });

  describe('createSkill', () => {
    it('calls api.post with skill name', async () => {
      const skill: Skill = { id: 's-3', name: 'billing', agentCount: 0 };
      api.post.mockReturnValue(of({ data: skill }));

      const result = await firstValueFrom(service.createSkill('billing'));

      expect(api.post).toHaveBeenCalledWith('tenant/skills', { name: 'billing' });
      expect(result.data).toEqual(skill);
    });
  });

  describe('renameSkill', () => {
    it('calls api.patch with skill id and name', async () => {
      const skill: Skill = { id: 's-1', name: 'billing-v2', agentCount: 3 };
      api.patch.mockReturnValue(of({ data: skill }));

      const result = await firstValueFrom(service.renameSkill('s-1', 'billing-v2'));

      expect(api.patch).toHaveBeenCalledWith('tenant/skills/s-1', { name: 'billing-v2' });
      expect(result.data).toEqual(skill);
    });
  });

  describe('deleteSkill', () => {
    it('calls api.delete with skill id', async () => {
      api.delete.mockReturnValue(of({ data: undefined }));

      const result = await firstValueFrom(service.deleteSkill('s-1'));

      expect(api.delete).toHaveBeenCalledWith('tenant/skills/s-1');
      expect(result.data).toBeUndefined();
    });
  });

  describe('setMemberSkills', () => {
    it('calls api.put with membership id and skill ids', async () => {
      api.put.mockReturnValue(of({ data: undefined }));

      const result = await firstValueFrom(service.setMemberSkills('m-1', ['s-1', 's-2']));

      expect(api.put).toHaveBeenCalledWith('tenant/members/m-1/skills', {
        skillIds: ['s-1', 's-2'],
      });
      expect(result.data).toBeUndefined();
    });
  });

  describe('acceptInvitation', () => {
    it('calls api.post with the token and payload', async () => {
      const payload: AcceptInvitationRequest = { displayName: 'Alice', password: 'secret123' };
      api.post.mockReturnValue(
        of({
          data: {
            id: 'u-1',
            email: 'alice@test.com',
            displayName: 'Alice',
            platformRole: null,
            platformPermissions: [],
            staffTenantPermissions: null,
            memberships: [],
          },
        }),
      );

      await firstValueFrom(service.acceptInvitation('token-123', payload));

      expect(api.post).toHaveBeenCalledWith('invitations/token-123/accept', payload);
    });
  });
});
