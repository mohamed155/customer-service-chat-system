import { HttpErrorResponse } from '@angular/common/http';
import { describeError, MAX_MESSAGE_CHARS, scrubText } from './scrub';

describe('scrubText', () => {
  it.each([
    [
      'the query and fragment of a URL',
      'Http failure response for https://app.test/api/v1/customers?email=a@b.com&token=abc#frag: 500',
      // The text after the query goes with it up to the next space, the ':' included.
      'Http failure response for https://app.test/api/v1/customers 500',
    ],
    [
      'the fragment of a URL that has no query',
      'redirected to https://app.test/callback#id=abc and back',
      'redirected to https://app.test/callback and back',
    ],
    ['the query of a path', 'at /invite/short?code=123 failed', 'at /invite/short failed'],
    [
      'a JWT',
      'bad token eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.c2lnbmF0dXJl here',
      'bad token [token] here',
    ],
    [
      'a bearer header',
      'sent Authorization: Bearer abc.def-ghi',
      'sent Authorization: [redacted] [token]',
    ],
    ['an e-mail address', 'no account for jane.doe+x@example.co.uk', 'no account for [email]'],
    [
      'a password in a query-like pair',
      'form failed password=hunter2&x=1',
      'form failed password=[redacted]&x=1',
    ],
    [
      'a value in JSON',
      '{"password":"hunter2","name":"Jane"}',
      '{"password":"[redacted]","name":"Jane"}',
    ],
    ['a cookie', 'cookie: app_session=abc123', 'cookie: [redacted]'],
    ['a UUID', 'customer 0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e0f not found', 'customer :id not found'],
    [
      'a long path segment (invite or verification link)',
      'route /invite/AbCdEfGhIjKlMnOpQrStUvWxYz012345 failed',
      'route /invite/:token failed',
    ],
    [
      'a long run of token characters',
      'key abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKL used',
      'key [redacted] used',
    ],
  ])('removes %s', (_what, input, expected) => {
    expect(scrubText(input, 1000)).toBe(expected);
  });

  it('keeps what makes a stack readable', () => {
    const stack =
      'TypeError: x is not a function\n    at Object.run (https://app.test/main-4TYQZ6FJ.js:10:20)\n    at chunk-ABCD1234.js:1:2';

    expect(scrubText(stack, 1000)).toBe(stack);
  });

  it('cuts to the limit and marks the cut', () => {
    const out = scrubText('word '.repeat(20), 10);

    expect(out).toHaveLength(10);
    expect(out.endsWith('…')).toBe(true);
  });
});

describe('describeError', () => {
  it('describes an ApiError by its code and keeps its request ID, never the backend message', () => {
    const description = describeError({
      code: 'validation_failed',
      status: 422,
      message: 'Email jane@example.com is already taken',
      requestId: 'req_0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e0f',
    });

    expect(description).toEqual({
      message: 'API error validation_failed (status 422)',
      requestId: 'req_0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e0f',
    });
  });

  it('describes an HttpErrorResponse by its status: no URL, no body', () => {
    const error = new HttpErrorResponse({
      status: 500,
      url: 'https://app.test/api/v1/x?token=abc',
      error: { password: 'hunter2' },
    });

    expect(describeError(error)).toEqual({ message: 'HTTP failure (status 500)' });
  });

  it('gives an Error its scrubbed message and stack', () => {
    const error = new Error('Could not load https://app.test/a?token=abc for jane@example.com');
    error.stack = 'Error: x\n    at https://app.test/main-ABCD1234.js?v=1:1:1';

    const description = describeError(error);

    expect(description.message).toBe('Error: Could not load https://app.test/a for [email]');
    expect(description.stack).toBe('Error: x\n    at https://app.test/main-ABCD1234.js');
  });

  it('describes a thrown value that is not an Error by its type only', () => {
    expect(describeError({ password: 'hunter2' }).message).toBe('Non-error value thrown (object)');
    expect(describeError(null).message).toBe('Non-error value thrown (null)');
    expect(describeError(42).message).toBe('Non-error value thrown (number)');
  });

  it('scrubs a thrown string and cuts a long message', () => {
    expect(describeError('failed for jane@example.com').message).toBe('failed for [email]');
    expect(describeError('word '.repeat(1000)).message).toHaveLength(MAX_MESSAGE_CHARS);
  });
});
