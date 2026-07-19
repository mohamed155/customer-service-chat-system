import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
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
});
