import { HttpErrorResponse } from '@angular/common/http';
import { ApiError, ApiErrorDetail } from '../api/api.models';

interface ErrorEnvelope {
  error?: { code?: unknown; message?: unknown; details?: unknown; request_id?: unknown };
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

export const mapHttpError = (error: unknown): ApiError => {
  if (error instanceof HttpErrorResponse) {
    if (
      error.status === 0 ||
      (typeof ProgressEvent !== 'undefined' && error.error instanceof ProgressEvent)
    )
      return { code: 'network_error', message: 'Network request failed', status: 0 };
    const body = error.error as ErrorEnvelope | null;
    const envelope = body && typeof body === 'object' ? body.error : undefined;
    if (envelope && typeof envelope.code === 'string' && typeof envelope.message === 'string') {
      const details = validDetails(envelope.details);
      return {
        code: envelope.code,
        message: envelope.message,
        status: error.status,
        ...(details ? { details } : {}),
        ...(typeof envelope.request_id === 'string' ? { requestId: envelope.request_id } : {}),
      };
    }
    return { code: 'unknown_error', message: 'Unexpected HTTP error', status: error.status };
  }
  return { code: 'unknown_error', message: 'Unexpected error', status: 0 };
};

export const userMessageFor = (error: ApiError): string => {
  if (error.code === 'network_error') return 'Check your connection and try again.';
  if (error.status === 401) return 'You do not have access to this action.';
  if (error.status === 403 && error.code === 'unauthorized')
    return "You don't have access to this tenant.";
  if (error.status === 403) return 'You do not have access to this action.';
  if (error.status === 404) return 'The requested item could not be found.';
  if (error.status === 429) return 'Too many requests. Please try again shortly.';
  if (error.status >= 500) return 'The service is temporarily unavailable. Please try again.';
  if (error.status >= 400)
    return 'The request could not be completed. Check your input and try again.';
  return 'Something went wrong. Please try again.';
};
