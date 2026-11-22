import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { SessionRefreshService } from './session-refresh.service';

describe('SessionRefreshService', () => {
  const URL = '/api/v1/auth/refresh';

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1/' } },
      ],
    });
  });

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('posts to the refresh endpoint and reports success', async () => {
    const result = firstValueFrom(TestBed.inject(SessionRefreshService).refresh());
    const req = TestBed.inject(HttpTestingController).expectOne(URL);
    expect(req.request.method).toBe('POST');
    req.flush(null, { status: 204, statusText: 'No Content' });
    expect(await result).toBe(true);
  });

  it('reports a refused refresh token as false, without throwing', async () => {
    const result = firstValueFrom(TestBed.inject(SessionRefreshService).refresh());
    TestBed.inject(HttpTestingController)
      .expectOne(URL)
      .flush({ code: 'unauthenticated' }, { status: 401, statusText: 'Unauthorized' });
    expect(await result).toBe(false);
  });

  it.each([429, 503])(
    'throws a %s, which says nothing about whether the session is over',
    async (status) => {
      const result = firstValueFrom(TestBed.inject(SessionRefreshService).refresh()).then(
        () => 0,
        (error: { status: number }) => error.status,
      );
      TestBed.inject(HttpTestingController)
        .expectOne(URL)
        .flush(null, { status, statusText: 'Trouble' });
      expect(await result).toBe(status);
    },
  );

  it('shares one request between callers that arrive while it is in flight', async () => {
    const service = TestBed.inject(SessionRefreshService);
    const first = firstValueFrom(service.refresh());
    const second = firstValueFrom(service.refresh());
    const third = firstValueFrom(service.refresh());

    // One call to the server, whatever the number of callers: the token rotates on every use.
    const req = TestBed.inject(HttpTestingController).expectOne(URL);
    req.flush(null, { status: 204, statusText: 'No Content' });

    expect(await Promise.all([first, second, third])).toEqual([true, true, true]);
  });

  it('starts a new refresh once the earlier one has finished', async () => {
    const service = TestBed.inject(SessionRefreshService);
    const http = TestBed.inject(HttpTestingController);

    const first = firstValueFrom(service.refresh());
    http.expectOne(URL).flush(null, { status: 204, statusText: 'No Content' });
    await first;

    const second = firstValueFrom(service.refresh());
    http.expectOne(URL).flush(null, { status: 204, statusText: 'No Content' });
    expect(await second).toBe(true);
  });
});
