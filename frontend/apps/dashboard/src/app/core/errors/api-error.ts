import { ApiError } from '../api/api.models';
import { mapHttpError } from './http-error.mapper';

/** Whether a value has the shape of an `ApiError` (what the HTTP layer hands to a failed call). */
export const isApiError = (value: unknown): value is ApiError =>
  typeof value === 'object' &&
  value !== null &&
  typeof (value as ApiError).code === 'string' &&
  typeof (value as ApiError).status === 'number';

/**
 * The `ApiError` of whatever a failed call threw: the HTTP layer's own, or one made from a raw
 * `HttpErrorResponse` or anything else. A store keeps this (not a message string) so a screen can
 * show the text for its `code` and the request reference (`requestId`) beside it.
 */
export const toApiError = (error: unknown): ApiError =>
  isApiError(error) ? error : mapHttpError(error);
