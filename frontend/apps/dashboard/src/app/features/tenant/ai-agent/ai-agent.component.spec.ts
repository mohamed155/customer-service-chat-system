import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, Subject, throwError } from 'rxjs';
import { AgentConfigResponse, AgentOptionsResponse } from '../../../core/api/ai-agent.models';
import { AiAgentApiService } from './ai-agent-api.service';
import { AiAgentComponent } from './ai-agent.component';

describe('AiAgentComponent', () => {
  let mockApi: {
    getAgent: ReturnType<typeof vi.fn>;
    getOptions: ReturnType<typeof vi.fn>;
    saveAgent: ReturnType<typeof vi.fn>;
  };

  const defaultConfig: AgentConfigResponse = {
    configured: true,
    agent: {
      id: 'agent-1',
      name: 'Helix',
      isDefault: false,
      avatar: { kind: 'preset', preset: 'bot-1', uploadUrl: null },
      tone: 'professional',
      systemPrompt: 'You are a helpful assistant.',
      businessRules: ['Be polite'],
      escalationRules: [],
      enabledChannels: ['web_chat'],
      providerSelection: { provider: 'openai', model: 'gpt-4', stale: false },
      version: 1,
      updatedAt: '2026-07-16T00:00:00Z',
    },
  };

  const defaultOptions: AgentOptionsResponse = {
    tones: ['professional', 'casual'],
    channels: ['web_chat', 'email'],
    avatarPresets: ['bot-1', 'bot-2'],
    providers: [],
    aiLayerDefault: { provider: null, model: null },
    promptMaxLength: 8000,
    limits: { businessRulesMax: 10, escalationRulesMax: 5 },
  };

  beforeEach(() => {
    mockApi = { getAgent: vi.fn(), getOptions: vi.fn(), saveAgent: vi.fn() };
    TestBed.configureTestingModule({
      imports: [AiAgentComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: AiAgentApiService, useValue: mockApi },
      ],
    });
  });

  afterEach(() => {
    TestBed.resetTestingModule();
  });

  function createFixture() {
    const fixture = TestBed.createComponent(AiAgentComponent);
    fixture.detectChanges();
    return fixture;
  }

  it('renders loading state while data is being fetched', () => {
    mockApi.getAgent.mockReturnValue(new Subject());
    mockApi.getOptions.mockReturnValue(new Subject());

    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
  });

  it('renders error state with retry button on load failure', () => {
    mockApi.getAgent.mockReturnValue(throwError(() => ({ message: 'Network error', status: 500 })));
    mockApi.getOptions.mockReturnValue(
      throwError(() => ({ message: 'Network error', status: 500 })),
    );

    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    const emptyState = fixture.nativeElement.querySelector('app-empty-state');
    expect(emptyState).toBeTruthy();
    expect(emptyState.textContent).toContain('Something went wrong');
  });

  it('renders the form with config data when loaded', async () => {
    mockApi.getAgent.mockReturnValue(of({ data: defaultConfig }));
    mockApi.getOptions.mockReturnValue(of({ data: defaultOptions }));

    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.textContent).toContain('Agent profile');
      expect(fixture.nativeElement.textContent).toContain('Professional');
    });
  });

  it('shows not-configured notice when configured is false', async () => {
    const unconfiguredConfig: AgentConfigResponse = {
      configured: false,
      agent: { ...defaultConfig.agent, name: '', systemPrompt: 'Default prompt' },
    };

    mockApi.getAgent.mockReturnValue(of({ data: unconfiguredConfig }));
    mockApi.getOptions.mockReturnValue(of({ data: defaultOptions }));

    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.textContent).toContain('Not yet configured');
    });
  });

  it('calls store.save() when save button is clicked', async () => {
    mockApi.getAgent.mockReturnValue(of({ data: defaultConfig }));
    mockApi.getOptions.mockReturnValue(of({ data: defaultOptions }));

    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.textContent).toContain('Behavior');
    });

    const saveSubject = new Subject<{ data: AgentConfigResponse }>();
    mockApi.saveAgent.mockReturnValue(saveSubject);

    const saveBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Save')!;
    saveBtn.click();

    expect(mockApi.saveAgent).toHaveBeenCalled();
  });

  it('switches tabs when a tab button is clicked', async () => {
    mockApi.getAgent.mockReturnValue(of({ data: defaultConfig }));
    mockApi.getOptions.mockReturnValue(of({ data: defaultOptions }));

    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    const promptTab = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('[role="tab"]'),
    ).find((b) => b.textContent?.trim() === 'Prompt')! as HTMLElement;
    promptTab.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.textContent).toContain('System prompt');
    });
  });
});
