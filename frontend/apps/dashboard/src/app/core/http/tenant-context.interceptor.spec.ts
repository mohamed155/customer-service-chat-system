import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { provideMockStore, MockStore } from '@ngrx/store/testing';
import { firstValueFrom } from 'rxjs';
import { APP_CONFIG, AppConfig } from '../config/app-config';
import { tenantContextInterceptor } from './tenant-context.interceptor';

const config: AppConfig = {
  apiBaseUrl: '/api/v1',
  appName: 'Dashboard',
  environmentName: 'development',
  enableNgRxDevtools: false,
  publicDashboardUrl: 'http://localhost:4200',
};

describe('tenantContextInterceptor', () => {
  let store: MockStore;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([tenantContextInterceptor])),
        provideHttpClientTesting(),
        { provide: APP_CONFIG, useValue: config },
        provideMockStore({
          initialState: { tenantContext: { activeTenant: null, status: 'idle' as const } },
        }),
      ],
    });
    store = TestBed.inject(MockStore);
  });

  it('attaches X-Tenant-ID for non-platform URLs when tenant is active', async () => {
    store.setState({
      tenantContext: {
        activeTenant: { id: 'tenant-1', name: 'Test', slug: 'test', status: 'active' },
        status: 'idle',
      },
    });

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/conversations'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/v1/conversations');
    expect(req.request.headers.get('X-Tenant-ID')).toBe('tenant-1');
    req.flush({});
    await promise;
  });

  it('does not attach X-Tenant-ID for /me paths', async () => {
    store.setState({
      tenantContext: {
        activeTenant: { id: 'tenant-1', name: 'Test', slug: 'test', status: 'active' },
        status: 'idle',
      },
    });

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/me'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/v1/me');
    expect(req.request.headers.has('X-Tenant-ID')).toBe(false);
    req.flush({});
    await promise;
  });

  it('does not attach X-Tenant-ID for /platform/ paths', async () => {
    store.setState({
      tenantContext: {
        activeTenant: { id: 'tenant-1', name: 'Test', slug: 'test', status: 'active' },
        status: 'idle',
      },
    });

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/platform/users'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/v1/platform/users');
    expect(req.request.headers.has('X-Tenant-ID')).toBe(false);
    req.flush({});
    await promise;
  });

  it('does not attach X-Tenant-ID when no active tenant', async () => {
    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/conversations'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/v1/conversations');
    expect(req.request.headers.has('X-Tenant-ID')).toBe(false);
    req.flush({});
    await promise;
  });
  describe('workspace calls whose path contains "me" (MOH-60)', () => {
    const withTenant = () =>
      store.setState({
        tenantContext: {
          activeTenant: { id: 'tenant-1', name: 'Test', slug: 'test', status: 'active' },
          status: 'idle',
        },
      });

    it.each([
      '/api/v1/tenant/members',
      '/api/v1/tenant/conversations/x/messages',
      '/api/v1/tenant/metrics/overview',
      '/api/v1/tenant/availability/me',
    ])('attaches X-Tenant-ID to %s', async (url) => {
      withTenant();
      const promise = firstValueFrom(TestBed.inject(HttpClient).get(url));
      const req = TestBed.inject(HttpTestingController).expectOne(url);
      expect(req.request.headers.get('X-Tenant-ID')).toBe('tenant-1');
      req.flush({});
      await promise;
    });

    it('still leaves /me and platform routes without the header', async () => {
      withTenant();
      for (const url of ['/api/v1/me', '/api/v1/platform/tenants']) {
        const promise = firstValueFrom(TestBed.inject(HttpClient).get(url));
        const req = TestBed.inject(HttpTestingController).expectOne(url);
        expect(req.request.headers.has('X-Tenant-ID')).toBe(false);
        req.flush({});
        await promise;
      }
    });

    it('does not send the workspace to a URL outside the API', async () => {
      withTenant();
      const url = 'https://third-party.test/track';
      const promise = firstValueFrom(TestBed.inject(HttpClient).get(url));
      const req = TestBed.inject(HttpTestingController).expectOne(url);
      expect(req.request.headers.has('X-Tenant-ID')).toBe(false);
      req.flush({});
      await promise;
    });

    it('follows a tenant switch without keeping old state per request', async () => {
      withTenant();
      let promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/tenant/members'));
      TestBed.inject(HttpTestingController).expectOne('/api/v1/tenant/members').flush({});
      await promise;

      store.setState({
        tenantContext: {
          activeTenant: { id: 'tenant-2', name: 'Two', slug: 'two', status: 'active' },
          status: 'idle',
        },
      });
      promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/tenant/members'));
      const req = TestBed.inject(HttpTestingController).expectOne('/api/v1/tenant/members');
      expect(req.request.headers.get('X-Tenant-ID')).toBe('tenant-2');
      req.flush({});
      await promise;
    });
  });
});
