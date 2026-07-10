import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom } from 'rxjs';
import { APP_CONFIG, AppConfig } from '../config/app-config';
import { authTokenInterceptor } from './auth-token.interceptor';

describe('authTokenInterceptor', () => {
  const config: AppConfig = {
    apiBaseUrl: '/api/v1',
    appName: 'Dashboard',
    environmentName: 'development',
    enableNgRxDevtools: false,
  };

  function configure(apiBaseUrl = config.apiBaseUrl): void {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([authTokenInterceptor])),
        provideHttpClientTesting(),
        { provide: APP_CONFIG, useValue: { ...config, apiBaseUrl } },
      ],
    });
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('sets credentials on requests targeting the configured API base URL', async () => {
    configure();

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/me'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/v1/me');

    expect(req.request.withCredentials).toBe(true);
    req.flush({});
    await promise;
  });

  it('sets credentials when an absolute request targets an absolute API base URL', async () => {
    configure('https://api.example.test/api/v1/');

    const promise = firstValueFrom(
      TestBed.inject(HttpClient).get('https://api.example.test/api/v1/auth/login'),
    );
    const req = TestBed.inject(HttpTestingController).expectOne(
      'https://api.example.test/api/v1/auth/login',
    );

    expect(req.request.withCredentials).toBe(true);
    req.flush({});
    await promise;
  });

  it('leaves non-API requests untouched', async () => {
    configure();

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/assets/i18n/en.json'));
    const req = TestBed.inject(HttpTestingController).expectOne('/assets/i18n/en.json');

    expect(req.request.withCredentials).toBe(false);
    req.flush({});
    await promise;
  });

  it('does not treat similarly prefixed paths as API requests', async () => {
    configure();

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v10/me'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/v10/me');

    expect(req.request.withCredentials).toBe(false);
    req.flush({});
    await promise;
  });

  it('leaves absolute requests to other origins untouched', async () => {
    configure('https://api.example.test/api/v1');

    const promise = firstValueFrom(
      TestBed.inject(HttpClient).get('https://cdn.example.test/api/v1/me'),
    );
    const req = TestBed.inject(HttpTestingController).expectOne(
      'https://cdn.example.test/api/v1/me',
    );

    expect(req.request.withCredentials).toBe(false);
    req.flush({});
    await promise;
  });
});
