import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { APP_CONFIG, AppConfig } from '../config/app-config';
import { requestIdInterceptor } from './request-id.interceptor';

const config: AppConfig = {
  apiBaseUrl: '/api/v1',
  appName: 'Dashboard',
  environmentName: 'development',
  enableNgRxDevtools: false,
  publicDashboardUrl: 'http://localhost:4200',
};

describe('requestIdInterceptor', () => {
  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([requestIdInterceptor])),
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
    return request.request.headers.get('X-Request-ID');
  };

  it('adds an X-Request-ID the backend will accept to an API call', () => {
    expect(send('/api/v1/tenant/members')).toMatch(/^req_[0-9a-f-]{36}$/);
  });

  it('gives each call its own ID', () => {
    expect(send('/api/v1/me')).not.toBe(send('/api/v1/me'));
  });

  it('adds none to a call to another server', () => {
    expect(send('https://third-party.test/api/v1/track')).toBeNull();
    expect(send('/assets/logo.svg')).toBeNull();
  });

  it('keeps an ID the call already carries', () => {
    expect(send('/api/v1/me', { 'X-Request-ID': 'req_given' })).toBe('req_given');
  });
});
