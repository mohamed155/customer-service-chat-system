import { HttpErrorResponse } from '@angular/common/http';
import { isApiError, toApiError } from './api-error';

describe('toApiError', () => {
  it('keeps an ApiError as it is, with its request ID', () => {
    const error = { code: 'conflict', message: 'x', status: 409, requestId: 'req_1' };
    expect(isApiError(error)).toBe(true);
    expect(toApiError(error)).toBe(error);
  });

  it('makes one from a raw HTTP error or anything else', () => {
    const http = new HttpErrorResponse({
      status: 503,
      error: { error: { code: 'internal_error', message: 'x', request_id: 'req_2' } },
    });
    expect(toApiError(http)).toMatchObject({
      code: 'internal_error',
      status: 503,
      requestId: 'req_2',
    });
    expect(toApiError(new Error('boom'))).toMatchObject({ code: 'unknown_error' });
    expect(toApiError(null)).toMatchObject({ code: 'unknown_error', status: 0 });
    expect(isApiError('text')).toBe(false);
  });
});
