import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { provideMockStore, MockStore } from '@ngrx/store/testing';
import { firstValueFrom } from 'rxjs';
import { tenantContextInterceptor } from './tenant-context.interceptor';

describe('tenantContextInterceptor', () => {
  let store: MockStore;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([tenantContextInterceptor])),
        provideHttpClientTesting(),
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

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/conversations'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/conversations');
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

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/me'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/me');
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

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/platform/users'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/platform/users');
    expect(req.request.headers.has('X-Tenant-ID')).toBe(false);
    req.flush({});
    await promise;
  });

  it('does not attach X-Tenant-ID when no active tenant', async () => {
    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/conversations'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/conversations');
    expect(req.request.headers.has('X-Tenant-ID')).toBe(false);
    req.flush({});
    await promise;
  });
});
