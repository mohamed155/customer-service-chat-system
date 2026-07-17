import { TestBed } from '@angular/core/testing';
import { Subject, of } from 'rxjs';
import { provideZonelessChangeDetection } from '@angular/core';
import { PromptBootstrapResponse, PromptSaveResponse } from '../../../../core/api/ai-agent.models';
import { PromptApiService } from './prompt-api.service';
import { PromptStore } from './prompt.store';
import { ApiError } from '../../../../core/api/api.models';

describe('PromptStore', () => {
  let mockApi: {
    getPrompt: ReturnType<typeof vi.fn>;
    savePrompt: ReturnType<typeof vi.fn>;
  };

  const mockBootstrap: PromptBootstrapResponse = {
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

  function configureStore() {
    TestBed.configureTestingModule({
      providers: [
        provideZonelessChangeDetection(),
        PromptStore,
        { provide: PromptApiService, useValue: mockApi },
      ],
    });
    return TestBed.inject(PromptStore);
  }

  beforeEach(() => {
    mockApi = { getPrompt: vi.fn(), savePrompt: vi.fn() };
  });

  it('initializes with default state', () => {
    mockApi.getPrompt.mockReturnValue(new Subject());
    const store = configureStore();
    expect(store.bootstrap()).toBeNull();
    expect(store.editorContent()).toBe('');
    expect(store.changeNote()).toBe('');
    expect(store.dirty()).toBe(false);
    expect(store.saving()).toBe(false);
    expect(store.error()).toBeNull();
    expect(store.conflict()).toBe(false);
    expect(store.fieldErrors()).toBeNull();
    expect(store.noOpNotice()).toBe(false);
  });

  it('loads bootstrap data on init', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    configureStore();

    TestBed.flushEffects();

    expect(mockApi.getPrompt).toHaveBeenCalledOnce();
  });

  it('populates editorContent after successful load', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const store = configureStore();
    TestBed.flushEffects();

    expect(store.loading()).toBe(false);
    expect(store.editorContent()).toBe(mockBootstrap.prompt.content);
    expect(store.bootstrap()).toEqual(mockBootstrap);
    expect(store.dirty()).toBe(false);
  });

  it('tracks dirty state on setContent', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const store = configureStore();
    TestBed.flushEffects();

    store.setContent('different content');
    expect(store.editorContent()).toBe('different content');
    expect(store.dirty()).toBe(true);

    store.setContent(mockBootstrap.prompt.content);
    expect(store.dirty()).toBe(false);
  });

  it('clears noOpNotice when content changes', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const store = configureStore();
    TestBed.flushEffects();

    store.setContent('new content');
    expect(store.noOpNotice()).toBe(false);
  });

  it('saves prompt and updates state on success', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const store = configureStore();
    TestBed.flushEffects();

    store.setContent('updated content');
    store.setChangeNote('refined wording');

    const saveSubject = new Subject<{ data: PromptSaveResponse }>();
    mockApi.savePrompt.mockReturnValue(saveSubject);

    store.save();
    expect(store.saving()).toBe(true);

    const saveResponse: PromptSaveResponse = {
      version: 5,
      created: true,
      updatedAt: '2026-07-16T12:00:00Z',
      updatedBy: 'Dana Ops',
    };
    saveSubject.next({ data: saveResponse });
    saveSubject.complete();

    expect(store.saving()).toBe(false);
    expect(store.dirty()).toBe(false);
    expect(store.changeNote()).toBe('');
    expect(store.noOpNotice()).toBe(false);
    expect(store.bootstrap()?.prompt.activeVersion).toBe(5);
    expect(store.bootstrap()?.prompt.content).toBe('updated content');
  });

  it('shows noOpNotice when save returns created: false', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const store = configureStore();
    TestBed.flushEffects();

    const saveSubject = new Subject<{ data: PromptSaveResponse }>();
    mockApi.savePrompt.mockReturnValue(saveSubject);

    store.save();

    const noopResponse: PromptSaveResponse = {
      version: 4,
      created: false,
      updatedAt: '2026-07-16T12:00:00Z',
      updatedBy: 'Dana Ops',
    };
    saveSubject.next({ data: noopResponse });
    saveSubject.complete();

    expect(store.saving()).toBe(false);
    expect(store.noOpNotice()).toBe(true);
  });

  it('handles 409 conflict on save', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const store = configureStore();
    TestBed.flushEffects();

    const saveSubject = new Subject<{ data: PromptSaveResponse }>();
    mockApi.savePrompt.mockReturnValue(saveSubject);

    store.save();

    const error: ApiError = { code: 'conflict', message: 'Version conflict', status: 409 };
    saveSubject.error(error);

    expect(store.saving()).toBe(false);
    expect(store.conflict()).toBe(true);
    expect(store.error()).toBe('Version conflict');
  });

  it('handles 422 validation errors on save', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const store = configureStore();
    TestBed.flushEffects();

    const saveSubject = new Subject<{ data: PromptSaveResponse }>();
    mockApi.savePrompt.mockReturnValue(saveSubject);

    store.save();

    const error: ApiError = {
      code: 'validation_failed',
      message: 'Validation failed',
      status: 422,
      details: [{ field: 'content', code: 'too_long', message: 'Content exceeds maximum length' }],
    };
    saveSubject.error(error);

    expect(store.saving()).toBe(false);
    expect(store.fieldErrors()).toEqual({
      content: ['Content exceeds maximum length'],
    });
  });

  it('handles generic error on save', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const store = configureStore();
    TestBed.flushEffects();

    const saveSubject = new Subject<{ data: PromptSaveResponse }>();
    mockApi.savePrompt.mockReturnValue(saveSubject);

    store.save();

    const error: ApiError = { code: 'server_error', message: 'Internal error', status: 500 };
    saveSubject.error(error);

    expect(store.saving()).toBe(false);
    expect(store.error()).toBe('Internal error');
  });

  it('dismissConflict clears conflict state', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const store = configureStore();
    TestBed.flushEffects();

    const saveSubject = new Subject<{ data: PromptSaveResponse }>();
    mockApi.savePrompt.mockReturnValue(saveSubject);

    store.save();
    saveSubject.error({ code: 'conflict', message: 'Conflict', status: 409 });

    expect(store.conflict()).toBe(true);
    store.dismissConflict();
    expect(store.conflict()).toBe(false);
  });

  it('dismissNoOpNotice clears noOpNotice state', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const store = configureStore();
    TestBed.flushEffects();

    const saveSubject = new Subject<{ data: PromptSaveResponse }>();
    mockApi.savePrompt.mockReturnValue(saveSubject);

    store.save();
    const noopResponse: PromptSaveResponse = {
      version: 4,
      created: false,
      updatedAt: '2026-07-16T12:00:00Z',
      updatedBy: 'Dana Ops',
    };
    saveSubject.next({ data: noopResponse });
    saveSubject.complete();

    expect(store.noOpNotice()).toBe(true);
    store.dismissNoOpNotice();
    expect(store.noOpNotice()).toBe(false);
  });

  it('setChangeNote updates change note', () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const store = configureStore();
    TestBed.flushEffects();

    store.setChangeNote('my change note');
    expect(store.changeNote()).toBe('my change note');
  });
});
