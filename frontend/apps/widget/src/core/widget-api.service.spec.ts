import { TestBed } from '@angular/core/testing';
import { HttpErrorResponse, provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { WidgetApiService, WIDGET_API_BASE } from './widget-api.service';
import { RateLimitedError, SessionExpiredError } from './models';

describe('WidgetApiService', () => {
  let service: WidgetApiService;
  let http: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        WidgetApiService,
        { provide: WIDGET_API_BASE, useValue: 'http://test' },
      ],
    });

    service = TestBed.inject(WidgetApiService);
    http = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    http.verify();
  });

  it('sends Authorization header on conversation calls', () => {
    service.getConversation('test-token').subscribe();
    const req = http.expectOne('http://test/widget/v1/conversation');
    expect(req.request.headers.get('Authorization')).toBe('Bearer test-token');
    req.flush({ data: { conversation: null } });
  });

  it('sends Authorization header on message send', () => {
    service.sendMessage('test-token', 'conv1', 'hello').subscribe();
    const req = http.expectOne('http://test/widget/v1/conversations/conv1/messages');
    expect(req.request.headers.get('Authorization')).toBe('Bearer test-token');
    expect(req.request.body).toEqual({ body: 'hello' });
    req.flush({ data: { message: { id: 'm1', sender: 'visitor', body: 'hello', createdAt: '' } } });
  });

  it('maps 429 to RateLimitedError', () => {
    service.createSession('wgt_1').subscribe({
      error: (err) => {
        expect(err).toBeInstanceOf(RateLimitedError);
      },
    });
    const req = http.expectOne('http://test/widget/v1/sessions');
    req.flush({}, { status: 429, statusText: 'Too Many Requests' });
  });

  it('maps 401 to SessionExpiredError', () => {
    service.getConversation('bad-token').subscribe({
      error: (err) => {
        expect(err).toBeInstanceOf(SessionExpiredError);
      },
    });
    const req = http.expectOne('http://test/widget/v1/conversation');
    req.flush({}, { status: 401, statusText: 'Unauthorized' });
  });

  describe('errors in either format (ADR-012 gap A2, MOH-200)', () => {
    const legacy = (code: string) => ({
      error: { code, message: 'raw', details: [], request_id: 'r' },
    });
    const problem = (status: number, code: string) => ({
      type: `urn:problem:${code}`,
      title: 'T',
      status,
      detail: 'raw',
      instance: '/widget/v1/sessions',
      code,
      request_id: 'r',
      errors: [],
    });
    const fail = (status: number, body: object | string | null, statusText: string) => {
      let caught: unknown;
      service.createSession('w1').subscribe({ error: (err) => (caught = err) });
      http.expectOne('http://test/widget/v1/sessions').flush(body, { status, statusText });
      return caught;
    };

    it('maps a 429 to RateLimitedError in each format and with no body', () => {
      for (const body of [legacy('rate_limited'), problem(429, 'rate_limited'), '<html></html>']) {
        expect(fail(429, body, 'Too Many Requests')).toBeInstanceOf(RateLimitedError);
      }
    });

    it('maps a 401 to SessionExpiredError in each format', () => {
      for (const body of [legacy('unauthenticated'), problem(401, 'unauthenticated')]) {
        expect(fail(401, body, 'Unauthorized')).toBeInstanceOf(SessionExpiredError);
      }
    });

    it('passes a 5xx through unchanged, whatever the body', () => {
      for (const body of [
        legacy('internal_error'),
        problem(500, 'internal_error'),
        '<html>Bad gateway</html>',
        null,
      ]) {
        const caught = fail(500, body, 'Internal Server Error');
        expect(caught).toBeInstanceOf(HttpErrorResponse);
        expect(caught).not.toBeInstanceOf(RateLimitedError);
        expect(caught).not.toBeInstanceOf(SessionExpiredError);
        expect((caught as HttpErrorResponse).status).toBe(500);
      }
    });
  });
});
