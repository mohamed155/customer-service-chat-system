import { HttpErrorResponse } from '@angular/common/http';
import { mapHttpError, userMessageFor } from './http-error.mapper';

describe('HTTP error mapping', () => {
  it('maps a backend envelope', () => {
    const result = mapHttpError(
      new HttpErrorResponse({
        status: 422,
        error: {
          error: {
            code: 'validation_failed',
            message: 'raw detail',
            details: [{ field: 'name', code: 'required', message: 'required' }],
            request_id: 'req-1',
          },
        },
      }),
    );
    expect(result).toMatchObject({ code: 'validation_failed', status: 422, requestId: 'req-1' });
    expect(userMessageFor(result)).not.toContain('raw detail');
  });
  it('maps network failure', () =>
    expect(mapHttpError(new HttpErrorResponse({ status: 0 })).code).toBe('network_error'));
  it('maps malformed HTTP bodies', () =>
    expect(mapHttpError(new HttpErrorResponse({ status: 500, error: 'broken' })).code).toBe(
      'unknown_error',
    ));
  it('maps unknown values', () =>
    expect(mapHttpError(new Error('boom')).code).toBe('unknown_error'));
  it('returns safe status copy and fallback', () => {
    expect(userMessageFor({ code: 'x', message: 'secret', status: 503 })).toContain('temporarily');
    expect(userMessageFor({ code: 'x', message: 'secret', status: 0 })).toBe(
      'Something went wrong. Please try again.',
    );
  });
});
