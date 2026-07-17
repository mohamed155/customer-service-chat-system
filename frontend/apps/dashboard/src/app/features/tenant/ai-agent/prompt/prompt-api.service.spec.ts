import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of } from 'rxjs';
import { provideZonelessChangeDetection } from '@angular/core';
import { ApiService } from '../../../../core/api/api.service';
import { PromptBootstrapResponse, PromptSavePayload } from '../../../../core/api/ai-agent.models';
import { PromptApiService } from './prompt-api.service';

describe('PromptApiService', () => {
  let service: PromptApiService;
  let api: {
    get: ReturnType<typeof vi.fn>;
    put: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    api = { get: vi.fn(), put: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        PromptApiService,
        provideZonelessChangeDetection(),
        { provide: ApiService, useValue: api },
      ],
    });
    service = TestBed.inject(PromptApiService);
  });

  describe('getPrompt', () => {
    it('calls api.get with tenant/ai/agent/prompt', async () => {
      const mockResponse: PromptBootstrapResponse = {
        prompt: {
          exists: true,
          activeVersion: 4,
          content: 'You are {{agent_name}}.',
          updatedAt: '2026-07-16T10:12:00Z',
          updatedBy: 'Dana Ops',
        },
        variables: [{ name: 'agent_name', description: 'The AI agent name', sample: 'Aria' }],
        limits: { maxContentLength: 8000, maxChangeNoteLength: 500 },
      };
      api.get.mockReturnValue(of({ data: mockResponse }));

      const result = await firstValueFrom(service.getPrompt());

      expect(api.get).toHaveBeenCalledWith('tenant/ai/agent/prompt');
      expect(result.data).toEqual(mockResponse);
    });
  });

  describe('savePrompt', () => {
    it('calls api.put with payload', async () => {
      const payload: PromptSavePayload = {
        content: 'You are {{agent_name}}.',
        changeNote: 'Updated greeting',
        baseVersion: 4,
      };
      const response = {
        data: {
          version: 5,
          created: true,
          updatedAt: '2026-07-16T12:00:00Z',
          updatedBy: 'Dana Ops',
        },
      };
      api.put.mockReturnValue(of(response));

      const result = await firstValueFrom(service.savePrompt(payload));

      expect(api.put).toHaveBeenCalledWith('tenant/ai/agent/prompt', payload);
      expect(result).toEqual(response);
    });
  });
});
