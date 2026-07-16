import { HttpClient } from '@angular/common/http';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of } from 'rxjs';
import { APP_CONFIG } from '../../../core/config/app-config';
import { ApiService } from '../../../core/api/api.service';
import { AiAgentApiService } from './ai-agent-api.service';

describe('AiAgentApiService', () => {
  let service: AiAgentApiService;
  let api: {
    get: ReturnType<typeof vi.fn>;
    put: ReturnType<typeof vi.fn>;
    post: ReturnType<typeof vi.fn>;
  };
  let http: {
    put: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    api = { get: vi.fn(), put: vi.fn(), post: vi.fn() };
    http = { put: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        AiAgentApiService,
        { provide: ApiService, useValue: api },
        { provide: HttpClient, useValue: http },
        {
          provide: APP_CONFIG,
          useValue: { apiBaseUrl: '/api/v1' },
        },
      ],
    });
    service = TestBed.inject(AiAgentApiService);
  });

  describe('getAgent', () => {
    it('calls api.get with tenant/ai/agent', async () => {
      const response = { data: { configured: false, agent: {} } };
      api.get.mockReturnValue(of(response));

      const result = await firstValueFrom(service.getAgent());

      expect(api.get).toHaveBeenCalledWith('tenant/ai/agent');
      expect(result).toEqual(response);
    });
  });

  describe('saveAgent', () => {
    it('calls api.put with payload', async () => {
      const payload = {
        name: 'Test Agent',
        avatar: { kind: 'preset' as const, preset: 'bot-1' },
        tone: 'professional',
        systemPrompt: 'You are a helpful assistant',
        businessRules: ['Be polite'],
        escalationRules: [],
        enabledChannels: ['web_chat'],
      };
      const response = { data: { configured: true, agent: {} } };
      api.put.mockReturnValue(of(response));

      const result = await firstValueFrom(service.saveAgent(payload));

      expect(api.put).toHaveBeenCalledWith('tenant/ai/agent', payload);
      expect(result).toEqual(response);
    });
  });

  describe('getOptions', () => {
    it('calls api.get with tenant/ai/agent/options', async () => {
      const response = {
        data: {
          tones: [],
          channels: [],
          avatarPresets: [],
          providers: [],
          aiLayerDefault: { provider: null, model: null },
          promptMaxLength: 2000,
          limits: { businessRulesMax: 10, escalationRulesMax: 5 },
        },
      };
      api.get.mockReturnValue(of(response));

      const result = await firstValueFrom(service.getOptions());

      expect(api.get).toHaveBeenCalledWith('tenant/ai/agent/options');
      expect(result).toEqual(response);
    });
  });

  describe('uploadAvatar', () => {
    it('sends blob via http.put with content-type header', async () => {
      const blob = new Blob(['fake-data'], { type: 'image/png' });
      const avatarResponse = {
        data: {
          avatar: { kind: 'upload' as const, preset: null, uploadUrl: '/avatars/1.png' },
          version: 1,
        },
      };
      http.put.mockReturnValue(
        of({
          body: avatarResponse.data,
          headers: { get: () => null },
        }),
      );

      const result = await firstValueFrom(service.uploadAvatar(blob, 'image/png'));

      expect(http.put).toHaveBeenCalledWith('/api/v1/tenant/ai/agent/avatar', blob, {
        headers: { 'Content-Type': 'image/png' },
        observe: 'response',
      });
      expect(result).toEqual(avatarResponse);
    });
  });

  describe('getAvatarUrl', () => {
    it('returns the avatar URL string', () => {
      const url = service.getAvatarUrl();
      expect(url).toBe('/api/v1/tenant/ai/agent/avatar');
    });
  });

  describe('setConversationAiHandling', () => {
    it('sends POST with conversation id and mode', async () => {
      const response = { data: undefined };
      api.post.mockReturnValue(of(response));

      const result = await firstValueFrom(
        service.setConversationAiHandling('conv-1', 'platform_ai'),
      );

      expect(api.post).toHaveBeenCalledWith('tenant/conversations/conv-1/ai-handling', {
        mode: 'platform_ai',
      });
      expect(result).toEqual(response);
    });

    it('sends POST with human mode', async () => {
      api.post.mockReturnValue(of({ data: undefined }));

      await firstValueFrom(service.setConversationAiHandling('conv-2', 'human'));

      expect(api.post).toHaveBeenCalledWith('tenant/conversations/conv-2/ai-handling', {
        mode: 'human',
      });
    });
  });
});
