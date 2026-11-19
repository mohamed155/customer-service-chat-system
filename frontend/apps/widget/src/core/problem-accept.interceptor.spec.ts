import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { ACCEPT_PROBLEM_JSON, problemAcceptInterceptor } from './problem-accept.interceptor';
import { WIDGET_API_BASE } from './widget-api.service';

describe('problemAcceptInterceptor', () => {
  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([problemAcceptInterceptor])),
        provideHttpClientTesting(),
        { provide: WIDGET_API_BASE, useValue: 'http://test' },
      ],
    });
  });

  afterEach(() => TestBed.inject(HttpTestingController).verify());

  const send = (url: string, headers: Record<string, string> = {}) => {
    TestBed.inject(HttpClient).get(url, { headers }).subscribe();
    const call = TestBed.inject(HttpTestingController).expectOne(url);
    call.flush({});
    return call.request;
  };

  it('asks the widget API for problem+json first and the legacy envelope at half the weight', () => {
    expect(ACCEPT_PROBLEM_JSON).toBe('application/problem+json, application/json;q=0.5');
    expect(send('http://test/widget/v1/config').headers.get('Accept')).toBe(ACCEPT_PROBLEM_JSON);
  });

  it('asks on a POST too', () => {
    TestBed.inject(HttpClient).post('http://test/widget/v1/sessions', {}).subscribe();
    const call = TestBed.inject(HttpTestingController).expectOne('http://test/widget/v1/sessions');
    expect(call.request.headers.get('Accept')).toBe(ACCEPT_PROBLEM_JSON);
    call.flush({});
  });

  it('keeps an Accept the caller chose', () => {
    expect(
      send('http://test/widget/v1/config', { Accept: 'text/plain' }).headers.get('Accept'),
    ).toBe('text/plain');
  });

  it('leaves other servers alone, including one whose address starts like the API base', () => {
    expect(send('https://elsewhere.example/data').headers.has('Accept')).toBe(false);
    expect(send('http://test.evil.example/widget/v1/config').headers.has('Accept')).toBe(false);
  });
});
