import { HttpErrorResponse, HttpHeaders } from '@angular/common/http';
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

describe('the request reference on a mapped error (MOH-64)', () => {
  const envelope = (request_id: string) => ({
    error: { code: 'internal_error', message: 'raw', request_id },
  });

  it('prefers the request_id in the error body', () => {
    const result = mapHttpError(
      new HttpErrorResponse({
        status: 500,
        headers: new HttpHeaders({ 'X-Request-ID': 'req_header' }),
        error: envelope('req_body'),
      }),
      'req_sent',
    );
    expect(result.requestId).toBe('req_body');
  });

  it('falls back to the X-Request-ID the server answered with when the body has none', () => {
    const result = mapHttpError(
      new HttpErrorResponse({
        status: 500,
        headers: new HttpHeaders({ 'X-Request-ID': 'req_header' }),
        error: envelope(''),
      }),
      'req_sent',
    );
    expect(result.requestId).toBe('req_header');
  });

  it('falls back to the ID the call went out with when the server sent none', () => {
    const gateway = new HttpErrorResponse({ status: 502, error: '<html>Bad gateway</html>' });
    expect(mapHttpError(gateway, 'req_sent')).toMatchObject({
      code: 'unknown_error',
      requestId: 'req_sent',
    });
  });

  it('keeps the sent ID when there was no answer at all', () => {
    expect(mapHttpError(new HttpErrorResponse({ status: 0 }), 'req_sent')).toMatchObject({
      code: 'network_error',
      requestId: 'req_sent',
    });
  });

  it('has no reference when nothing carried one', () => {
    expect(mapHttpError(new HttpErrorResponse({ status: 502 })).requestId).toBeUndefined();
  });
});
