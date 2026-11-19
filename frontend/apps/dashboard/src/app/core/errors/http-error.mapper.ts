import { HttpErrorResponse } from '@angular/common/http';
import { TimeoutError } from 'rxjs';
import { ApiError, ApiErrorDetail } from '../api/api.models';

/** The legacy envelope: `{ error: { code, message, details, request_id } }`. */
interface ErrorEnvelope {
  error?: { code?: unknown; message?: unknown; details?: unknown; request_id?: unknown };
}

/**
 * An RFC 9457 problem document (`application/problem+json`, ADR-012 gap A2): `code`, `detail`,
 * `request_id`, `status` and `errors[]` of `{ field, code }`. The dashboard asks for it and still
 * reads the legacy envelope, which the backend sends to a client that does not ask (until MOH-201).
 */
interface ProblemDocument {
  code?: unknown;
  detail?: unknown;
  status?: unknown;
  request_id?: unknown;
  errors?: unknown;
}

/** What both formats say, in the shape the rest of the mapper uses. */
interface ParsedBody {
  readonly code: string;
  readonly message: string;
  readonly details?: ApiErrorDetail[];
  readonly requestId?: string;
}

const validDetails = (value: unknown): ApiErrorDetail[] | undefined => {
  if (!Array.isArray(value)) return undefined;
  const details = value.filter(
    (item): item is ApiErrorDetail =>
      typeof item === 'object' &&
      item !== null &&
      typeof (item as ApiErrorDetail).code === 'string' &&
      typeof (item as ApiErrorDetail).message === 'string',
  );
  return details.length ? details : undefined;
};

/**
 * A problem document's field errors. They carry no sentence (a screen says it by `code`, ADR-016),
 * so `message` is empty: the envelope's `message` was never shown either.
 */
const problemDetails = (value: unknown): ApiErrorDetail[] | undefined => {
  if (!Array.isArray(value)) return undefined;
  const details = value.flatMap((item): ApiErrorDetail[] =>
    typeof item === 'object' &&
    item !== null &&
    typeof (item as { field?: unknown }).field === 'string' &&
    typeof (item as { code?: unknown }).code === 'string'
      ? [
          {
            field: (item as { field: string }).field,
            code: (item as { code: string }).code,
            message: '',
          },
        ]
      : [],
  );
  return details.length ? details : undefined;
};

const nonEmpty = (value: unknown): string | undefined =>
  typeof value === 'string' && value !== '' ? value : undefined;

/** The error a response body describes, in either format, or `undefined` for any other body. */
const parseBody = (body: unknown): ParsedBody | undefined => {
  if (!body || typeof body !== 'object') return undefined;

  const envelope = (body as ErrorEnvelope).error;
  if (envelope && typeof envelope.code === 'string' && typeof envelope.message === 'string') {
    const details = validDetails(envelope.details);
    const requestId = nonEmpty(envelope.request_id);
    return {
      code: envelope.code,
      message: envelope.message,
      ...(details ? { details } : {}),
      ...(requestId ? { requestId } : {}),
    };
  }

  const problem = body as ProblemDocument;
  if (
    typeof problem.code === 'string' &&
    typeof problem.detail === 'string' &&
    typeof problem.status === 'number'
  ) {
    const details = problemDetails(problem.errors);
    const requestId = nonEmpty(problem.request_id);
    return {
      code: problem.code,
      message: problem.detail,
      ...(details ? { details } : {}),
      ...(requestId ? { requestId } : {}),
    };
  }
  return undefined;
};

/**
 * `sentRequestId` is the ID the call went out with. The reference is, in order: the server's
 * `request_id` in the error body, the `X-Request-ID` it answered with, the one sent. The last one
 * covers a failure with no answer at all (offline, a gateway timeout).
 */
export const mapHttpError = (error: unknown, sentRequestId?: string | null): ApiError => {
  if (error instanceof TimeoutError) {
    return {
      code: 'timeout',
      message: 'The request timed out',
      status: 0,
      ...(sentRequestId ? { requestId: sentRequestId } : {}),
    };
  }
  if (error instanceof HttpErrorResponse) {
    if (
      error.status === 0 ||
      (typeof ProgressEvent !== 'undefined' && error.error instanceof ProgressEvent)
    )
      return {
        code: 'network_error',
        message: 'Network request failed',
        status: 0,
        ...(sentRequestId ? { requestId: sentRequestId } : {}),
      };
    const answeredWith = error.headers?.get('X-Request-ID') ?? undefined;
    const requestId = answeredWith ?? sentRequestId ?? undefined;
    const parsed = parseBody(error.error);
    if (parsed) {
      const reference = parsed.requestId ?? requestId;
      return {
        code: parsed.code,
        message: parsed.message,
        // The status the response had, not the one the body claims.
        status: error.status,
        ...(parsed.details ? { details: parsed.details } : {}),
        ...(reference ? { requestId: reference } : {}),
      };
    }
    return {
      code: 'unknown_error',
      message: 'Unexpected HTTP error',
      status: error.status,
      ...(requestId ? { requestId } : {}),
    };
  }
  return { code: 'unknown_error', message: 'Unexpected error', status: 0 };
};
