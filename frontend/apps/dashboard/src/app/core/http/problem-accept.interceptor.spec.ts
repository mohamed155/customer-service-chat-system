import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { APP_CONFIG, AppConfig } from '../config/app-config';
import { ACCEPT_PROBLEM_JSON, problemAcceptInterceptor } from './problem-accept.interceptor';

const config: AppConfig = {
  apiBaseUrl: '/api/v1',
  appName: 'Dashboard',
  environmentName: 'development',
  publicDashboardUrl: 'http://localhost:4200',
};

describe('problemAcceptInterceptor', () => {
  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([problemAcceptInterceptor])),
        provideHttpClientTesting(),
        { provide: APP_CONFIG, useValue: config },
      ],
    });
  });

  afterEach(() => TestBed.inject(HttpTestingController).verify());

  const send = (url: string, headers: Record<string, string> = {}) => {
    TestBed.inject(HttpClient).get(url, { headers }).subscribe();
    const request = TestBed.inject(HttpTestingController).expectOne(url);
    request.flush({});
    return request.request;
  };

  it('asks the API for problem+json first and the legacy envelope at half the weight', () => {
    expect(ACCEPT_PROBLEM_JSON).toBe('application/problem+json, application/json;q=0.5');
    expect(send('/api/v1/tenant/customers').headers.get('Accept')).toBe(ACCEPT_PROBLEM_JSON);
  });

  it('asks on every method', () => {
    const http = TestBed.inject(HttpClient);
    const testing = TestBed.inject(HttpTestingController);
    http.post('/api/v1/tenant/customers', {}).subscribe();
    http.patch('/api/v1/tenant/customers/1', {}).subscribe();
    http.delete('/api/v1/tenant/customers/1').subscribe();
    for (const call of testing.match(() => true)) {
      expect(call.request.headers.get('Accept')).toBe(ACCEPT_PROBLEM_JSON);
      call.flush({});
    }
  });

  it('keeps an Accept the caller chose', () => {
    expect(send('/api/v1/files/1', { Accept: 'application/pdf' }).headers.get('Accept')).toBe(
      'application/pdf',
    );
  });

  it('leaves calls to other servers alone', () => {
    expect(send('https://elsewhere.example/data').headers.has('Accept')).toBe(false);
  });
});
