import { TestBed } from '@angular/core/testing';
import { HttpErrorResponse, provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting } from '@angular/common/http/testing';
import { WidgetStore, REPLY_TIMEOUT_MS } from './widget.store';
import { WIDGET_API_BASE, WidgetApiService } from './widget-api.service';
import { SessionStore } from './session.store';
import {
  WidgetConfig,
  WidgetEvent,
  PendingFeedback,
  WidgetFeedback,
  RateLimitedError,
} from './models';
import { of, throwError } from 'rxjs';

function makeConfig(): WidgetConfig {
  return {
    widgetId: 'test',
    displayName: 'Test',
    primaryColor: '#000',
    welcomeMessage: 'Hi',
    position: 'bottom-right',
    theme: 'light',
    enabled: true,
  };
}

describe('WidgetStore', () => {
  let store: WidgetStore;
  let sessionStore: SessionStore;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        // Keep any unstubbed request in memory instead of hitting the network.
        provideHttpClientTesting(),
        WidgetStore,
        SessionStore,
        { provide: WIDGET_API_BASE, useValue: 'http://test' },
      ],
    });

    store = TestBed.inject(WidgetStore);
    sessionStore = TestBed.inject(SessionStore);

    vi.spyOn(sessionStore, 'getToken').mockReturnValue('test-token');
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('accumulates ai.delta into streamingText', () => {
    const delta1: WidgetEvent = { type: 'ai.delta', text: 'Hello', messageId: null };
    const delta2: WidgetEvent = { type: 'ai.delta', text: ' world', messageId: null };

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (store as any).handleSseEvent(delta1);
    expect(store.streamingText()).toBe('Hello');

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (store as any).handleSseEvent(delta2);
    expect(store.streamingText()).toBe('Hello world');
  });

  it('replaces streaming text on message.created', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (store as any).handleSseEvent({
      type: 'ai.delta',
      text: 'Hello',
      messageId: null,
    });

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (store as any).handleSseEvent({
      type: 'message.created',
      message: {
        id: 'm1',
        sender: 'assistant',
        body: 'Hello',
        createdAt: new Date().toISOString(),
      },
    });

    expect(store.streamingText()).toBe('');
    expect(store.messages().length).toBe(1);
  });

  it('uiState transitions to responding on send', () => {
    store.setConfig(makeConfig());
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (store as any).conversationSignal.set({
      id: 'conv1',
      handling: 'ai',
      teamOnline: true,
      endedNote: false,
      messages: [],
    });

    vi.spyOn(TestBed.inject(WidgetApiService), 'sendMessage').mockReturnValue(
      of({ id: 'm1', sender: 'visitor', body: 'hello', createdAt: '' }),
    );

    store.sendMessage('hello', 'conv1');
    expect(store.uiState()).toBe('responding');
  });

  it('reply timeout transitions to error', () => {
    store.setConfig(makeConfig());
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (store as any).conversationSignal.set({
      id: 'conv1',
      handling: 'ai',
      teamOnline: true,
      endedNote: false,
      messages: [],
    });

    store.sendMessage('hello', 'conv1');
    expect(store.uiState()).toBe('responding');

    vi.advanceTimersByTime(REPLY_TIMEOUT_MS + 1000);
    expect(store.uiState()).toBe('error');
  });

  describe('feedbackState', () => {
    it('returns "none" when no pending feedback', () => {
      expect(store.feedbackState()).toBe('none');
    });

    it('returns "prompt" when pending feedback and not dismissed', () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (store as any).pendingFeedbackSignal.set({
        conversationId: 'conv1',
        endedAt: '2026-07-19T12:00:00Z',
      } as PendingFeedback);

      expect(store.feedbackState()).toBe('prompt');
    });

    it('returns "collapsed" when pending feedback is dismissed', () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (store as any).pendingFeedbackSignal.set({
        conversationId: 'conv1',
        endedAt: '2026-07-19T12:00:00Z',
      } as PendingFeedback);

      store.dismissFeedback();

      expect(store.feedbackState()).toBe('collapsed');
    });

    it('returns "submitted" after feedback is submitted', () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (store as any).feedbackSignal.set({
        rating: 4,
        comment: null,
        submittedAt: '2026-07-19T12:00:00Z',
      } as WidgetFeedback);

      expect(store.feedbackState()).toBe('submitted');
    });
  });

  describe('submitFeedback', () => {
    it('forwards comment to WidgetApiService.submitFeedback', () => {
      const apiService = TestBed.inject(WidgetApiService);
      const submitSpy = vi
        .spyOn(apiService, 'submitFeedback')
        .mockReturnValue(of({ rating: 5, comment: 'Great!', submittedAt: '2026-07-19T12:00:00Z' }));

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (store as any).pendingFeedbackSignal.set({
        conversationId: 'conv1',
        endedAt: '2026-07-19T12:00:00Z',
      } as PendingFeedback);

      store.submitFeedback(5, 'Great!');

      expect(submitSpy).toHaveBeenCalledWith('test-token', 'conv1', 5, 'Great!');
    });
  });

  describe('a failed send (ADR-012 gap A2, MOH-200)', () => {
    const body409 = (code: string) => ({
      legacy: { error: { code, message: 'raw', details: [], request_id: 'r' } },
      problem: {
        type: `urn:problem:${code}`,
        title: 'T',
        status: 409,
        detail: 'raw',
        instance: '/widget/v1/conversations/conv1/messages',
        code,
        request_id: 'r',
        errors: [],
      },
    });

    function sendFailing(error: HttpErrorResponse) {
      const apiService = TestBed.inject(WidgetApiService);
      vi.spyOn(apiService, 'sendMessage').mockReturnValue(throwError(() => error));
      const pending = vi.spyOn(apiService, 'getPendingFeedback').mockReturnValue(of(null));
      store.setConfig(makeConfig());
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (store as any).conversationSignal.set({
        id: 'conv1',
        handling: 'ai',
        teamOnline: true,
        endedNote: false,
        messages: [],
      });
      store.sendMessage('hello', 'conv1');
      return pending;
    }

    it('treats a closed conversation the same in either format', () => {
      for (const format of ['legacy', 'problem'] as const) {
        const pending = sendFailing(
          new HttpErrorResponse({ status: 409, error: body409('conversation_closed')[format] }),
        );
        expect(pending, format).toHaveBeenCalled();
        expect(store.uiState(), format).not.toBe('error');
      }
    });

    it('shows the generic error for a 409 that names another code', () => {
      for (const format of ['legacy', 'problem'] as const) {
        const pending = sendFailing(
          new HttpErrorResponse({ status: 409, error: body409('conflict')[format] }),
        );
        expect(pending, format).not.toHaveBeenCalled();
        expect(store.uiState(), format).toBe('error');
      }
    });

    it('shows the generic error for a 5xx, with a body in either format or none', () => {
      for (const error of [
        { error: { code: 'internal_error', message: 'x', details: [], request_id: 'r' } },
        { code: 'internal_error', detail: 'x', status: 500, request_id: 'r', errors: [] },
        '<html>Bad gateway</html>',
        null,
      ]) {
        sendFailing(new HttpErrorResponse({ status: 500, error }));
        expect(store.uiState()).toBe('error');
      }
    });

    it('shows the rate-limited state for a 429', () => {
      const apiService = TestBed.inject(WidgetApiService);
      vi.spyOn(apiService, 'sendMessage').mockReturnValue(throwError(() => new RateLimitedError()));
      store.setConfig(makeConfig());
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (store as any).conversationSignal.set({
        id: 'conv1',
        handling: 'ai',
        teamOnline: true,
        endedNote: false,
        messages: [],
      });
      store.sendMessage('hello', 'conv1');
      expect(store.uiState()).toBe('rate-limited');
    });
  });

  describe('409 handling triggers checkPendingFeedback', () => {
    it('calls checkPendingFeedback when sendMessage returns 409', () => {
      const apiService = TestBed.inject(WidgetApiService);
      vi.spyOn(apiService, 'sendMessage').mockReturnValue(
        throwError(() => new HttpErrorResponse({ status: 409 })),
      );
      vi.spyOn(apiService, 'getPendingFeedback').mockReturnValue(of(null));

      store.setConfig(makeConfig());
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (store as any).conversationSignal.set({
        id: 'conv1',
        handling: 'ai',
        teamOnline: true,
        endedNote: false,
        messages: [],
      });

      store.sendMessage('hello', 'conv1');
      expect(apiService.getPendingFeedback).toHaveBeenCalled();
    });
  });
});
