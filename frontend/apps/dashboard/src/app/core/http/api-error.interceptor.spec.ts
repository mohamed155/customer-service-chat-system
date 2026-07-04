import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom } from 'rxjs';
import { ApiError } from '../api/api.models';
import { apiErrorInterceptor } from './api-error.interceptor';
import { authTokenInterceptor } from './auth-token.interceptor';

describe('HTTP interceptors', () => {
  it('keeps auth as a no-op and normalizes failures', async () => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([authTokenInterceptor, apiErrorInterceptor])),
        provideHttpClientTesting(),
      ],
    });
    const requestPromise = firstValueFrom(TestBed.inject(HttpClient).get('/test')).catch(
      (error: ApiError) => error,
    );
    const request = TestBed.inject(HttpTestingController).expectOne('/test');
    expect(request.request.headers.has('Authorization')).toBe(false);
    request.flush(
      { error: { code: 'not_found', message: 'raw' } },
      { status: 404, statusText: 'Not Found' },
    );
    expect(await requestPromise).toMatchObject({ code: 'not_found', status: 404 });
  });
});
