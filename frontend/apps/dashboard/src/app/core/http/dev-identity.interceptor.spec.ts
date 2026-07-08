import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { devIdentityInterceptor } from './dev-identity.interceptor';

describe('devIdentityInterceptor', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('attaches X-Dev-User-Id in development mode when localStorage has the value', async () => {
    localStorage.setItem('app.devUserId', 'dev-user-42');
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([devIdentityInterceptor])),
        provideHttpClientTesting(),
        { provide: APP_CONFIG, useValue: { environmentName: 'development' } },
      ],
    });

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/conversations'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/conversations');
    expect(req.request.headers.get('X-Dev-User-Id')).toBe('dev-user-42');
    req.flush({});
    await promise;
  });

  it('does not attach X-Dev-User-Id in development mode when localStorage is empty', async () => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([devIdentityInterceptor])),
        provideHttpClientTesting(),
        { provide: APP_CONFIG, useValue: { environmentName: 'development' } },
      ],
    });

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/conversations'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/conversations');
    expect(req.request.headers.has('X-Dev-User-Id')).toBe(false);
    req.flush({});
    await promise;
  });

  it('does not attach X-Dev-User-Id in production mode', async () => {
    localStorage.setItem('app.devUserId', 'dev-user-42');
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([devIdentityInterceptor])),
        provideHttpClientTesting(),
        { provide: APP_CONFIG, useValue: { environmentName: 'production' } },
      ],
    });

    const promise = firstValueFrom(TestBed.inject(HttpClient).get('/api/conversations'));
    const req = TestBed.inject(HttpTestingController).expectOne('/api/conversations');
    expect(req.request.headers.has('X-Dev-User-Id')).toBe(false);
    req.flush({});
    await promise;
  });
});
