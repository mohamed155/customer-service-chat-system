import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { ApiService } from './api.service';

describe('ApiService', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        {
          provide: APP_CONFIG,
          useValue: {
            apiBaseUrl: '/api/v1',
            appName: 'Test',
            environmentName: 'development',
            enableNgRxDevtools: false,
          },
        },
      ],
    }),
  );
  it('prefixes the base URL and captures request IDs', async () => {
    const resultPromise = firstValueFrom(
      TestBed.inject(ApiService).get<{ id: number }>('/items/1'),
    );
    TestBed.inject(HttpTestingController)
      .expectOne('/api/v1/items/1')
      .flush({ id: 1 }, { headers: { 'X-Request-Id': 'req-2' } });
    expect(await resultPromise).toEqual({ data: { id: 1 }, requestId: 'req-2' });
  });
  it('serializes list query parameters', () => {
    TestBed.inject(ApiService)
      .list('/items', { limit: 20, cursor: 'next', order: 'asc', q: 'hello' })
      .subscribe();
    const request = TestBed.inject(HttpTestingController).expectOne(
      (candidate) => candidate.url === '/api/v1/items',
    );
    expect(request.request.params.get('limit')).toBe('20');
    expect(request.request.params.get('cursor')).toBe('next');
    expect(request.request.params.get('order')).toBe('asc');
    expect(request.request.params.get('q')).toBe('hello');
    request.flush({ items: [], nextCursor: null, hasMore: false });
  });
});
