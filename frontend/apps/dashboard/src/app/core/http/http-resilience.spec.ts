import { HttpErrorResponse, HttpHeaders } from '@angular/common/http';
import {
  BACKOFF_BASE_MS,
  BACKOFF_CAP_MS,
  MAX_RETRY_AFTER_MS,
  retryAfterMs,
  retryDelayMs,
} from './http-resilience';

const failure = (status: number, headers: Record<string, string> = {}) =>
  new HttpErrorResponse({ status, headers: new HttpHeaders(headers) });

const NEVER_WAIT = () => 0;
const ALWAYS_MAX = () => 0.999999;

describe('retryAfterMs', () => {
  it('reads delta-seconds', () => {
    expect(retryAfterMs('3')).toBe(3000);
    expect(retryAfterMs('0')).toBe(0);
    expect(retryAfterMs('1.5')).toBe(1500);
  });

  it('reads an HTTP date relative to now', () => {
    const now = Date.parse('2026-11-12T10:00:00Z');
    expect(retryAfterMs('Thu, 12 Nov 2026 10:00:04 GMT', now)).toBe(4000);
    expect(retryAfterMs('Thu, 12 Nov 2026 09:00:00 GMT', now)).toBe(0);
  });

  it('is null for nothing, junk or a negative number', () => {
    expect(retryAfterMs(null)).toBeNull();
    expect(retryAfterMs('')).toBeNull();
    expect(retryAfterMs('soon')).toBeNull();
    expect(retryAfterMs('-5')).toBeNull();
  });
});

describe('retryDelayMs', () => {
  it.each([0, 502, 503, 504])('retries status %s', (status) => {
    expect(retryDelayMs(failure(status), 1, NEVER_WAIT)).toBe(0);
  });

  it.each([400, 401, 403, 404, 409, 422, 500])('never retries status %s', (status) => {
    expect(retryDelayMs(failure(status), 1, NEVER_WAIT)).toBeNull();
  });

  it('never retries something that is not an HTTP error', () => {
    expect(retryDelayMs(new Error('boom'), 1, NEVER_WAIT)).toBeNull();
  });

  it('backs off exponentially with full jitter: between 0 and the capped step', () => {
    const ceilings = [1, 2, 3, 4, 5, 6].map((attempt) =>
      Math.min(BACKOFF_CAP_MS, BACKOFF_BASE_MS * 2 ** (attempt - 1)),
    );
    ceilings.forEach((ceiling, i) => {
      expect(retryDelayMs(failure(503), i + 1, NEVER_WAIT)).toBe(0);
      expect(retryDelayMs(failure(503), i + 1, ALWAYS_MAX)).toBe(ceiling - 1);
      expect(retryDelayMs(failure(503), i + 1, () => 0.5)).toBe(Math.floor(ceiling / 2));
    });
    expect(ceilings[ceilings.length - 1]).toBe(BACKOFF_CAP_MS);
  });

  it('waits for Retry-After as sent on a 503, with no jitter', () => {
    expect(retryDelayMs(failure(503, { 'Retry-After': '2' }), 1, ALWAYS_MAX)).toBe(2000);
  });

  it('does not wait for a Retry-After longer than the limit: the error is shown', () => {
    const tooLong = String(MAX_RETRY_AFTER_MS / 1000 + 1);
    expect(retryDelayMs(failure(503, { 'Retry-After': tooLong }), 1, NEVER_WAIT)).toBeNull();
  });

  it('retries a 429 only when the server says when', () => {
    expect(retryDelayMs(failure(429, { 'Retry-After': '1' }), 1, NEVER_WAIT)).toBe(1000);
    expect(retryDelayMs(failure(429), 1, NEVER_WAIT)).toBeNull();
    expect(retryDelayMs(failure(429, { 'Retry-After': '999' }), 1, NEVER_WAIT)).toBeNull();
  });
});
