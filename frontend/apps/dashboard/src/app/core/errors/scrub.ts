import { HttpErrorResponse } from '@angular/common/http';
import { ApiError } from '../api/api.models';

/**
 * What may leave the browser in an error report (MOH-168).
 *
 * A report carries a message, a stack, a route and a few IDs. Nothing else from the page goes in:
 * no form values, no request or response bodies, no cookies, no storage. The text that does go in
 * is scrubbed here first, because messages and stacks are written by libraries and by code that
 * never expected to be sent anywhere: an `HttpErrorResponse` message names the full URL with its
 * query, a validation error may quote a value, a stack names the files of the page.
 *
 * Scrubbing is best effort, so the rule above (send little) comes first and this is the second
 * line. Every pattern errs on the side of removing too much.
 */

/** Names whose value is never sent: `name=value`, `name: value` and `"name":"value"`. */
const SENSITIVE_KEYS =
  '(?:password|passwd|pwd|secret|token|authorization|cookie|set-cookie|session|otp|code|apikey|api_key|api-key|credential|signature|csrf|xsrf)';

const PATTERNS: readonly [RegExp, string][] = [
  // A JWT: three base64url parts, the first of which is a JSON header.
  [/\beyJ[\w-]+\.[\w-]+\.[\w-]*/g, '[token]'],
  [/\bBearer\s+[^\s,;'"]+/gi, 'Bearer [token]'],
  [/\bBasic\s+[A-Za-z0-9+/=]{8,}/g, 'Basic [token]'],
  // A URL loses its query and its fragment: they carry tokens, search terms and e-mail addresses.
  [/(\bhttps?:\/\/[^\s?#'"`)<>]*)[?#][^\s'"`)<>]*/g, '$1'],
  // The same for a bare path (`/invite/abc?x=1`).
  [/(\/[\w\-./:%~]*)\?[^\s'"`)<>]*/g, '$1'],
  [/\b[\w.+-]+@[\w-]+(?:\.[\w-]+)+\b/g, '[email]'],
  // `"password":"x"` and `'token': 'x'`
  [
    new RegExp(`(["']?${SENSITIVE_KEYS}["']?\\s*[:=]\\s*)(["'])(?:(?!\\2).)*\\2`, 'gi'),
    '$1$2[redacted]$2',
  ],
  // `password=x` and `authorization: x` up to the next separator
  [new RegExp(`(\\b${SENSITIVE_KEYS}\\s*[:=]\\s*)[^\\s,;&"'}]+`, 'gi'), '$1[redacted]'],
  // A UUID in a path or a message.
  [/\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi, ':id'],
  // A path segment that is long and made of token characters (invite and verification links).
  [/(\/)[A-Za-z0-9_-]{24,}(?=$|[/\s'"`)<>:,;])/g, '$1:token'],
  // Any other long run of token characters (keys, hashes, base64 blobs).
  [/\b[A-Za-z0-9+/_-]{40,}={0,2}/g, '[redacted]'],
];

/** Scrub `text` and cut it to `max` characters. */
export function scrubText(text: string, max: number): string {
  let out = text;
  for (const [pattern, replacement] of PATTERNS) {
    out = out.replace(pattern, replacement);
  }
  return out.length > max ? `${out.slice(0, max - 1)}…` : out;
}

export const MAX_MESSAGE_CHARS = 500;
export const MAX_STACK_CHARS = 2_000;

export interface ErrorDescription {
  readonly message: string;
  readonly stack?: string;
  readonly requestId?: string;
}

const isApiError = (error: unknown): error is ApiError =>
  typeof error === 'object' && error !== null && 'code' in error && 'status' in error;

/**
 * The message, stack and request ID of anything that was thrown. Only a real `Error` contributes
 * its own text. An `ApiError` is described by its code, never by the backend's message; an
 * `HttpErrorResponse` by its status, never by its body or URL; any other value by its type.
 */
export function describeError(error: unknown): ErrorDescription {
  if (isApiError(error)) {
    return {
      message: scrubText(`API error ${error.code} (status ${error.status})`, MAX_MESSAGE_CHARS),
      requestId: typeof error.requestId === 'string' ? error.requestId : undefined,
    };
  }
  if (error instanceof HttpErrorResponse) {
    return { message: `HTTP failure (status ${error.status})` };
  }
  if (error instanceof Error) {
    return {
      message: scrubText(`${error.name}: ${error.message}`, MAX_MESSAGE_CHARS),
      stack: error.stack ? scrubText(error.stack, MAX_STACK_CHARS) : undefined,
    };
  }
  if (typeof error === 'string') {
    return { message: scrubText(error, MAX_MESSAGE_CHARS) };
  }
  return { message: `Non-error value thrown (${error === null ? 'null' : typeof error})` };
}
