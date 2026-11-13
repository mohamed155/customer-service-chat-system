import { ApiError } from '../api/api.models';
import { marker } from '../i18n/marker';

// Errors are shown by their `code`, never by the backend's `message` (ADR-016 section 5, ADR-012:
// "clients branch on `code`, never on `detail` text"). This maps an `ApiError` to a catalog key in
// the shared catalog (`errors.*`); a screen shows it with `{{ key | transloco }}`.

/**
 * Every error `code` the identity and two-step endpoints answer with, the codes every endpoint
 * shares, and the ones the browser makes up (no answer, no JSON). The spec of this file reads the
 * backend source and fails when a code is added there and not listed here, so a new one cannot be
 * forgotten (MOH-156).
 */
export const ERROR_CODE_KEYS: Readonly<Record<string, string>> = {
  // From the browser (`unknown_error`, an answer that is not our envelope, is told by its status)
  network_error: marker('errors.networkError'),
  timeout: marker('errors.timeout'),
  // From every endpoint (kernel `ApiError`)
  validation_failed: marker('errors.requestFailed'),
  unauthenticated: marker('errors.accessDenied'),
  unauthorized: marker('errors.tenantAccessDenied'),
  not_found: marker('errors.notFound'),
  conflict: marker('errors.requestFailed'),
  gone: marker('errors.requestFailed'),
  unprocessable: marker('errors.requestFailed'),
  rate_limited: marker('errors.tooManyRequests'),
  internal_error: marker('errors.unavailable'),
  // From the invitation endpoint (tenancy `invitations.rs`)
  membership_disabled: marker('errors.membershipDisabled'),
  // From the two-step verification endpoints (identity `step_up.rs`)
  mfa_enrolment_required: marker('errors.accessDenied'),
  mfa_step_up_required: marker('errors.accessDenied'),
};

/** A screen's own wording for a code or a status, which wins over the general one. */
export type ErrorKeyOverrides = Readonly<Record<string | number, string>>;

/**
 * The catalog key for an error: the screen's own key for its code or status, else the key of the
 * code, else one for its status class, else a generic one. An unknown code never shows the
 * backend's text.
 */
export function errorMessageKey(error: ApiError, overrides: ErrorKeyOverrides = {}): string {
  const own = overrides[error.code] ?? overrides[error.status];
  if (own) return own;
  const byCode = ERROR_CODE_KEYS[error.code];
  if (byCode) return byCode;
  if (error.status === 401) return marker('errors.accessDenied');
  if (error.status === 403) return marker('errors.accessDenied');
  if (error.status === 404) return marker('errors.notFound');
  if (error.status === 429) return marker('errors.tooManyRequests');
  if (error.status >= 500) return marker('errors.unavailable');
  if (error.status >= 400) return marker('errors.requestFailed');
  return marker('errors.generic');
}

/**
 * The `code` of a field error in an API error's `details` (`{ field, code, message }`): the kernel
 * sends `required`, `invalid_value`, `invalid_format` and `invalid_range`. Any other code gets the
 * generic text; the detail's `message` is never shown.
 */
export const FIELD_CODE_KEYS: Readonly<Record<string, string>> = {
  required: marker('errors.fieldRequired'),
  invalid_value: marker('errors.fieldInvalid'),
  invalid_format: marker('errors.fieldInvalid'),
  invalid_range: marker('errors.fieldInvalid'),
};

export function fieldErrorKey(code: string): string {
  return FIELD_CODE_KEYS[code] ?? marker('errors.fieldInvalid');
}
