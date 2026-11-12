import {
  HttpClient,
  HttpErrorResponse,
  HttpInterceptorFn,
  provideHttpClient,
  withInterceptors,
} from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { catchError, firstValueFrom, throwError } from 'rxjs';
import { ApiError } from '../api/api.models';
import { mapHttpError } from '../errors/http-error.mapper';
import { StepUpService } from '../auth/step-up.service';
import { stepUpInterceptor } from './step-up.interceptor';

const STEP_UP_BODY = { error: { code: 'mfa_step_up_required', message: 'Enter a code' } };

// What `apiErrorInterceptor` does for this purpose: the interceptor under test sits outside it,
// so it sees the mapped error, not the raw HTTP one.
const mapErrors: HttpInterceptorFn = (request, next) =>
  next(request).pipe(
    catchError((error: unknown) =>
      throwError(() => (error instanceof HttpErrorResponse ? mapHttpError(error) : error)),
    ),
  );

describe('stepUpInterceptor', () => {
  let prompt: ReturnType<typeof vi.fn>;
  let http: HttpClient;
  let backend: HttpTestingController;

  beforeEach(() => {
    prompt = vi.fn();
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([stepUpInterceptor, mapErrors])),
        provideHttpClientTesting(),
        { provide: StepUpService, useValue: { prompt } },
      ],
    });
    http = TestBed.inject(HttpClient);
    backend = TestBed.inject(HttpTestingController);
  });

  afterEach(() => backend.verify());

  const stepUpRequired = (url: string): void => {
    backend.expectOne(url).flush(STEP_UP_BODY, { status: 401, statusText: 'Unauthorized' });
  };

  it('passes a successful response straight through', async () => {
    const result = firstValueFrom(http.get('/api/x'));
    backend.expectOne('/api/x').flush({ ok: true });
    await expect(result).resolves.toEqual({ ok: true });
    expect(prompt).not.toHaveBeenCalled();
  });

  it('does not prompt for any other error', async () => {
    const failed = firstValueFrom(http.get('/api/x')).catch((e: ApiError) => e.status);
    backend.expectOne('/api/x').flush({}, { status: 403, statusText: 'Forbidden' });
    await expect(failed).resolves.toBe(403);
    expect(prompt).not.toHaveBeenCalled();
  });

  it('does not prompt for a plain 401 that is not a step-up request', async () => {
    const failed = firstValueFrom(http.get('/api/x')).catch((e: ApiError) => e.code);
    backend
      .expectOne('/api/x')
      .flush(
        { error: { code: 'unauthenticated', message: 'Sign in' } },
        { status: 401, statusText: 'Unauthorized' },
      );
    await expect(failed).resolves.toBe('unauthenticated');
    expect(prompt).not.toHaveBeenCalled();
  });

  it('asks for a code and sends the original request once more when it is proven', async () => {
    prompt.mockResolvedValue(true);
    const result = firstValueFrom(http.put('/api/secret', { value: 1 }));
    stepUpRequired('/api/secret');
    await vi.waitFor(() => expect(prompt).toHaveBeenCalledTimes(1));

    const retry = await vi.waitFor(() => backend.expectOne('/api/secret'));
    expect(retry.request.method).toBe('PUT');
    expect(retry.request.body).toEqual({ value: 1 });
    retry.flush({ saved: true });
    await expect(result).resolves.toEqual({ saved: true });
  });

  it('retries only once: a second step-up answer reaches the caller', async () => {
    prompt.mockResolvedValue(true);
    const failed = firstValueFrom(http.get('/api/secret')).catch((e: ApiError) => e.code);
    stepUpRequired('/api/secret');
    const retry = await vi.waitFor(() => backend.expectOne('/api/secret'));
    retry.flush(STEP_UP_BODY, { status: 401, statusText: 'Unauthorized' });
    await expect(failed).resolves.toBe('mfa_step_up_required');
    expect(prompt).toHaveBeenCalledTimes(1);
  });

  it('gives the caller the original error when the user cancels', async () => {
    prompt.mockResolvedValue(false);
    const failed = firstValueFrom(http.get('/api/secret')).catch((e: ApiError) => e.code);
    stepUpRequired('/api/secret');
    await expect(failed).resolves.toBe('mfa_step_up_required');
    backend.expectNone('/api/secret');
  });
});
