import { TestBed } from '@angular/core/testing';
import { of, Subject } from 'rxjs';
import { provideZonelessChangeDetection } from '@angular/core';
import { ApiError } from '../../../core/api/api.models';
import {
  AgentConfigPayload,
  AgentConfigResponse,
  AgentOptionsResponse,
} from '../../../core/api/ai-agent.models';
import { AiAgentApiService } from './ai-agent-api.service';
import { AiAgentStore } from './ai-agent.store';

describe('AiAgentStore', () => {
  let mockApi: {
    getAgent: ReturnType<typeof vi.fn>;
    getOptions: ReturnType<typeof vi.fn>;
    saveAgent: ReturnType<typeof vi.fn>;
  };

  const mockConfig: AgentConfigResponse = {
    configured: true,
    agent: {
      id: 'agent-1',
      name: 'Helix',
      isDefault: false,
      avatar: { kind: 'preset', preset: 'bot-1', uploadUrl: null },
      tone: 'professional',
      activePrompt: {
        version: 4,
        updatedAt: '2026-07-16T10:12:00Z',
        updatedBy: 'Dana Ops',
        excerpt: 'You are {{agent_name}}.',
      },
      businessRules: ['Be polite'],
      escalationRules: [
        {
          id: 'er-1',
          name: 'Anger',
          trigger: 'topic_keywords',
          keywords: ['angry'],
          requiredSkillIds: ['skill-1'],
          brokenSkillRefs: [],
        },
      ],
      enabledChannels: ['web_chat'],
      providerSelection: { provider: 'openai', model: 'gpt-4', stale: false },
      version: 1,
      updatedAt: '2026-07-16T00:00:00Z',
    },
  };

  const mockOptions: AgentOptionsResponse = {
    tones: ['professional', 'casual'],
    channels: ['web_chat', 'email'],
    avatarPresets: ['bot-1', 'bot-2'],
    providers: [],
    aiLayerDefault: { provider: null, model: null },
    promptMaxLength: 8000,
    limits: { businessRulesMax: 10, escalationRulesMax: 5 },
  };

  function configureStore() {
    TestBed.configureTestingModule({
      providers: [
        provideZonelessChangeDetection(),
        AiAgentStore,
        { provide: AiAgentApiService, useValue: mockApi },
      ],
    });
    return TestBed.inject(AiAgentStore);
  }

  beforeEach(() => {
    mockApi = { getAgent: vi.fn(), getOptions: vi.fn(), saveAgent: vi.fn() };
  });

  it('initializes with default state and loads on init', () => {
    mockApi.getAgent.mockReturnValue(new Subject());
    mockApi.getOptions.mockReturnValue(new Subject());
    const store = configureStore();

    expect(store.config()).toBeNull();
    expect(store.options()).toBeNull();
    expect(store.saving()).toBe(false);
    expect(store.error()).toBeNull();
    expect(store.conflict()).toBe(false);
    expect(store.fieldErrors()).toBeNull();
    expect(store.activeTab()).toBe('behavior');
    expect(store.isConfigured()).toBe(false);
    expect(store.hasConflict()).toBe(false);
    expect(store.brokenSkillRefs()).toEqual([]);
    expect(store.staleProviderSelection()).toBe(false);
  });

  it('loads config and options on init', () => {
    mockApi.getAgent.mockReturnValue(of({ data: mockConfig }));
    mockApi.getOptions.mockReturnValue(of({ data: mockOptions }));
    configureStore();

    TestBed.flushEffects();

    expect(mockApi.getAgent).toHaveBeenCalledOnce();
    expect(mockApi.getOptions).toHaveBeenCalledOnce();
  });

  it('populates config and options after successful load', () => {
    mockApi.getAgent.mockReturnValue(of({ data: mockConfig }));
    mockApi.getOptions.mockReturnValue(of({ data: mockOptions }));
    const store = configureStore();

    TestBed.flushEffects();

    expect(store.loading()).toBe(false);
    expect(store.config()).toEqual(mockConfig);
    expect(store.options()).toEqual(mockOptions);
    expect(store.error()).toBeNull();
  });

  it('saves agent config via save()', () => {
    mockApi.getAgent.mockReturnValue(new Subject());
    mockApi.getOptions.mockReturnValue(new Subject());
    const store = configureStore();
    TestBed.flushEffects();

    const saveSubject = new Subject<{ data: AgentConfigResponse }>();
    mockApi.saveAgent.mockReturnValue(saveSubject);

    const payload: AgentConfigPayload = {
      name: 'Helix',
      avatar: { kind: 'preset', preset: 'bot-1' },
      tone: 'professional',
      businessRules: ['Be polite'],
      escalationRules: [],
      enabledChannels: ['web_chat'],
      version: 1,
    };

    store.save(payload);
    expect(store.saving()).toBe(true);

    saveSubject.next({ data: mockConfig });
    saveSubject.complete();

    expect(store.saving()).toBe(false);
    expect(store.config()).toEqual(mockConfig);
    expect(mockApi.saveAgent).toHaveBeenCalledWith(payload);
  });

  it('handles 409 conflict on save', () => {
    mockApi.getAgent.mockReturnValue(new Subject());
    mockApi.getOptions.mockReturnValue(new Subject());
    const store = configureStore();
    TestBed.flushEffects();

    const error: ApiError = { code: 'conflict', message: 'Version conflict', status: 409 };
    const saveSubject = new Subject<{ data: AgentConfigResponse }>();
    mockApi.saveAgent.mockReturnValue(saveSubject);

    const payload: AgentConfigPayload = {
      name: 'Helix',
      avatar: { kind: 'preset', preset: 'bot-1' },
      tone: 'professional',
      businessRules: [],
      escalationRules: [],
      enabledChannels: [],
    };

    store.save(payload);
    expect(store.saving()).toBe(true);

    saveSubject.error(error);

    expect(store.saving()).toBe(false);
    expect(store.conflict()).toBe(true);
    expect(store.error()).toBe('Version conflict');
  });

  it('handles 422 validation errors on save', () => {
    mockApi.getAgent.mockReturnValue(new Subject());
    mockApi.getOptions.mockReturnValue(new Subject());
    const store = configureStore();
    TestBed.flushEffects();

    const error: ApiError = {
      code: 'validation_error',
      message: 'Invalid input',
      status: 422,
      details: [
        { field: 'name', code: 'required', message: 'Name is required' },
        { field: 'tone', code: 'invalid', message: 'Invalid tone' },
      ],
    };
    const saveSubject = new Subject<{ data: AgentConfigResponse }>();
    mockApi.saveAgent.mockReturnValue(saveSubject);

    const payload: AgentConfigPayload = {
      name: '',
      avatar: { kind: 'preset', preset: 'bot-1' },
      tone: 'invalid',
      businessRules: [],
      escalationRules: [],
      enabledChannels: [],
    };

    store.save(payload);
    expect(store.saving()).toBe(true);

    saveSubject.error(error);

    expect(store.saving()).toBe(false);
    expect(store.fieldErrors()).toEqual({
      name: ['Name is required'],
      tone: ['Invalid tone'],
    });
  });

  it('switches active tab via setTab()', () => {
    mockApi.getAgent.mockReturnValue(new Subject());
    mockApi.getOptions.mockReturnValue(new Subject());
    const store = configureStore();
    TestBed.flushEffects();

    expect(store.activeTab()).toBe('behavior');

    store.setTab('prompt');
    expect(store.activeTab()).toBe('prompt');

    store.setTab('escalation');
    expect(store.activeTab()).toBe('escalation');

    store.setTab('behavior');
    expect(store.activeTab()).toBe('behavior');
  });
});
