import { HttpErrorResponse, HttpHeaders } from '@angular/common/http';
import { TimeoutError } from 'rxjs';
import { errorMessageKey } from './error-message';
import { mapHttpError } from './http-error.mapper';

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
    // The screen shows the text for the code, never the backend's sentence.
    expect(errorMessageKey(result)).toBe('errors.requestFailed');
  });
  it('maps network failure', () =>
    expect(mapHttpError(new HttpErrorResponse({ status: 0 })).code).toBe('network_error'));
  it('maps malformed HTTP bodies', () =>
    expect(mapHttpError(new HttpErrorResponse({ status: 500, error: 'broken' })).code).toBe(
      'unknown_error',
    ));
  it('maps unknown values', () =>
    expect(mapHttpError(new Error('boom')).code).toBe('unknown_error'));
  it('returns safe status wording and a generic fallback', () => {
    expect(errorMessageKey({ code: 'x', message: 'secret', status: 503 })).toBe(
      'errors.unavailable',
    );
    expect(errorMessageKey({ code: 'x', message: 'secret', status: 0 })).toBe('errors.generic');
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

describe('a timeout (MOH-166)', () => {
  it('maps to a typed error with a message for the person', () => {
    const result = mapHttpError(new TimeoutError(), 'req_sent');

    expect(result).toMatchObject({ code: 'timeout', status: 0, requestId: 'req_sent' });
    expect(errorMessageKey(result)).toBe('errors.timeout');
  });

  it('is not shown as a lost connection', () => {
    expect(errorMessageKey(mapHttpError(new TimeoutError()))).not.toBe('errors.networkError');
  });
});

describe('the two error formats (ADR-012 gap A2, MOH-199)', () => {
  const legacy = (
    code: string,
    message: string,
    details: unknown[] = [],
    request_id = 'req-1',
  ) => ({
    error: { code, message, details, request_id },
  });
  const problem = (
    status: number,
    code: string,
    detail: string,
    errors: unknown[] = [],
    request_id = 'req-1',
  ) => ({
    type: `urn:problem:${code}`,
    title: detail,
    status,
    detail,
    instance: '/api/v1/tenant/customers',
    code,
    request_id,
    errors,
  });
  const response = (status: number, error: unknown, headers: Record<string, string> = {}) =>
    new HttpErrorResponse({ status, error, headers: new HttpHeaders(headers) });

  it('maps a 422 with field errors the same way in each format', () => {
    const fromLegacy = mapHttpError(
      response(
        422,
        legacy('validation_failed', 'Validation failed', [
          { field: 'name', code: 'required', message: 'Name is required' },
          { field: 'email', code: 'invalid_format', message: 'Not an email address' },
        ]),
      ),
    );
    const fromProblem = mapHttpError(
      response(
        422,
        problem(422, 'validation_failed', 'Validation failed', [
          { field: 'name', code: 'required' },
          { field: 'email', code: 'invalid_format' },
        ]),
      ),
    );

    for (const result of [fromLegacy, fromProblem]) {
      expect(result).toMatchObject({
        code: 'validation_failed',
        message: 'Validation failed',
        status: 422,
        requestId: 'req-1',
      });
      // A screen reads a field and a code; it never shows the sentence.
      expect(result.details?.map(({ field, code }) => ({ field, code }))).toEqual([
        { field: 'name', code: 'required' },
        { field: 'email', code: 'invalid_format' },
      ]);
    }
    expect(errorMessageKey(fromProblem)).toBe(errorMessageKey(fromLegacy));
  });

  it('takes the status from the response, not from the body', () => {
    const result = mapHttpError(response(409, problem(200, 'conflict', 'Taken')));
    expect(result.status).toBe(409);
  });

  it('maps a 5xx the same way in each format, with nothing but its public message', () => {
    const fromLegacy = mapHttpError(
      response(500, legacy('internal_error', 'Internal server error', [], 'req_srv')),
    );
    const fromProblem = mapHttpError(
      response(500, problem(500, 'internal_error', 'Internal server error', [], 'req_srv')),
    );
    for (const result of [fromLegacy, fromProblem]) {
      expect(result).toEqual({
        code: 'internal_error',
        message: 'Internal server error',
        status: 500,
        requestId: 'req_srv',
      });
    }
    expect(errorMessageKey(fromProblem)).toBe(errorMessageKey(fromLegacy));
  });

  it('maps a 429 with Retry-After the same way in each format', () => {
    const headers = { 'Retry-After': '42' };
    const fromLegacy = mapHttpError(
      response(429, legacy('rate_limited', 'Too many attempts'), headers),
    );
    const fromProblem = mapHttpError(
      response(429, problem(429, 'rate_limited', 'Too many attempts'), headers),
    );
    expect(fromProblem).toEqual(fromLegacy);
    expect(fromProblem).toMatchObject({ code: 'rate_limited', status: 429 });
  });

  it('falls back to the answered or sent request ID when the problem has none', () => {
    const headers = { 'X-Request-ID': 'req_header' };
    expect(
      mapHttpError(response(404, problem(404, 'not_found', 'Gone', [], ''), headers), 'req_sent')
        .requestId,
    ).toBe('req_header');
    expect(
      mapHttpError(response(404, problem(404, 'not_found', 'Gone', [], '')), 'req_sent').requestId,
    ).toBe('req_sent');
  });

  it('ignores context that is not a field error', () => {
    const result = mapHttpError(
      response(409, {
        ...problem(409, 'conflict', 'Active escalation', [{ note: 'no field or code' }]),
        details: [{ escalationId: 'e1' }],
      }),
    );
    expect(result.details).toBeUndefined();
    expect(result.code).toBe('conflict');
  });

  it('maps a non-JSON body to the generic error', () => {
    for (const body of ['<html>Bad gateway</html>', '', null]) {
      expect(mapHttpError(response(502, body))).toMatchObject({
        code: 'unknown_error',
        status: 502,
      });
    }
  });

  it('maps a JSON body in neither format to the generic error', () => {
    for (const body of [
      { message: 'nope' },
      { code: 'x' },
      // Looks like a problem but has no status or no detail: not trusted.
      { code: 'x', detail: 'y' },
      { code: 'x', status: 400 },
      { error: 'a string' },
      { error: { code: 1, message: 2 } },
      [1, 2, 3],
    ]) {
      expect(mapHttpError(response(400, body)).code).toBe('unknown_error');
    }
  });
});
