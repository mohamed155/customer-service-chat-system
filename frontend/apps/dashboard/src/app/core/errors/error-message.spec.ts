import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { ApiError } from '../api/api.models';
import sharedEnglish from '../../../../public/i18n/shared/en.json';
import { ERROR_CODE_KEYS, FIELD_CODE_KEYS, errorMessageKey, fieldErrorKey } from './error-message';
import { userMessageFor } from './http-error.mapper';

const lookup = (key: string): unknown =>
  key
    .split('.')
    .reduce<unknown>((node, part) => (node as Record<string, unknown>)?.[part], sharedEnglish);

const error = (code: string, status: number): ApiError => ({
  code,
  status,
  message: 'TEXT FROM THE BACKEND',
});

/** The backend source is the list of codes this app can be given; a test reads it (MOH-156). */
const backend = (path: string): string =>
  readFileSync(resolve(process.cwd(), '..', 'backend', 'crates', path), 'utf8');

/** Codes the identity and two-step endpoints, and every other endpoint through `ApiError`, send. */
function backendCodes(): Set<string> {
  const codes = new Set<string>();
  const kernel = backend('shared/kernel/src/lib.rs');
  for (const m of kernel.matchAll(/new_with_code\(\s*StatusCode::[A-Z_]+,\s*"([a-z_]+)"/g)) {
    codes.add(m[1]);
  }
  for (const m of kernel.matchAll(/code:\s*"([a-z_]+)"\.to_owned\(\)/g)) codes.add(m[1]);
  const stepUp = backend('modules/identity/src/step_up.rs');
  for (const m of stepUp.matchAll(/pub const [A-Z_]+: &str = "([a-z_]+)";/g)) codes.add(m[1]);
  const invitations = backend('modules/tenancy/src/invitations.rs');
  for (const m of invitations.matchAll(/new_with_code\(\s*StatusCode::[A-Z_]+,\s*"([a-z_]+)"/g)) {
    codes.add(m[1]);
  }
  return codes;
}

describe('errorMessageKey', () => {
  it('has a key in the English catalog for every code in the table', () => {
    for (const [code, key] of Object.entries(ERROR_CODE_KEYS)) {
      expect(typeof lookup(key), `${code} -> ${key}`).toBe('string');
    }
  });

  it('lists every error code the backend identity, two-step, invitation and kernel code can send', () => {
    const codes = backendCodes();
    // The scan finds the codes it should: a regex that matched nothing would pass vacuously.
    expect([...codes]).toEqual(
      expect.arrayContaining([
        'validation_failed',
        'unauthenticated',
        'unauthorized',
        'not_found',
        'conflict',
        'gone',
        'unprocessable',
        'mfa_enrolment_required',
        'mfa_step_up_required',
        'membership_disabled',
      ]),
    );
    const missing = [...codes].filter((code) => !(code in ERROR_CODE_KEYS));
    expect(missing, `Add these to ERROR_CODE_KEYS and the catalog: ${missing.join(', ')}`).toEqual(
      [],
    );
  });

  it('also lists the codes the browser makes up, and the two the kernel builds by hand', () => {
    for (const code of ['network_error', 'timeout', 'rate_limited', 'internal_error']) {
      expect(ERROR_CODE_KEYS, code).toHaveProperty(code);
    }
  });

  it('never returns the backend text: every result is a key of the catalog', () => {
    const samples: ApiError[] = [
      ...Object.keys(ERROR_CODE_KEYS).map((code) => error(code, 400)),
      error('some_new_code', 400),
      error('some_new_code', 401),
      error('some_new_code', 403),
      error('some_new_code', 404),
      error('some_new_code', 429),
      error('some_new_code', 500),
      error('some_new_code', 0),
      error('unknown_error', 502),
    ];
    for (const sample of samples) {
      const key = errorMessageKey(sample);
      expect(key).not.toContain('BACKEND');
      expect(typeof lookup(key), `${sample.code}/${sample.status} -> ${key}`).toBe('string');
    }
  });

  it('shows a generic message for an unknown code, by its status class', () => {
    expect(errorMessageKey(error('some_new_code', 400))).toBe('errors.requestFailed');
    expect(errorMessageKey(error('some_new_code', 404))).toBe('errors.notFound');
    expect(errorMessageKey(error('some_new_code', 429))).toBe('errors.tooManyRequests');
    expect(errorMessageKey(error('some_new_code', 503))).toBe('errors.unavailable');
    expect(errorMessageKey(error('some_new_code', 0))).toBe('errors.generic');
    expect(errorMessageKey(error('unknown_error', 502))).toBe('errors.unavailable');
  });

  it('lets a screen word its own code or status, the code first', () => {
    const overrides = { 422: 'x.byStatus', unprocessable: 'x.byCode' };
    expect(errorMessageKey(error('unprocessable', 422), overrides)).toBe('x.byCode');
    expect(errorMessageKey(error('other', 422), overrides)).toBe('x.byStatus');
    expect(errorMessageKey(error('other', 400), overrides)).toBe('errors.requestFailed');
  });

  it('does not take a property of Object for a code, a field code or an override', () => {
    for (const code of ['constructor', 'toString', 'valueOf', '__proto__', 'hasOwnProperty']) {
      expect(errorMessageKey(error(code, 400)), code).toBe('errors.requestFailed');
      expect(errorMessageKey(error(code, 400), {}), code).toBe('errors.requestFailed');
      expect(fieldErrorKey(code), code).toBe('errors.fieldInvalid');
      expect(typeof userMessageFor(error(code, 500)), code).toBe('string');
    }
  });

  it('keeps the old English helper in step with the catalog', () => {
    for (const [code, status] of [
      ['network_error', 0],
      ['timeout', 0],
      ['unauthenticated', 401],
      ['unauthorized', 403],
      ['not_found', 404],
      ['rate_limited', 429],
      ['internal_error', 500],
      ['conflict', 409],
      ['x', 0],
    ] as const) {
      const sample = error(code, status);
      expect(userMessageFor(sample)).toBe(lookup(errorMessageKey(sample)));
    }
  });
});

describe('fieldErrorKey', () => {
  it('maps the field codes the kernel sends, and anything else to the generic text', () => {
    expect(fieldErrorKey('required')).toBe('errors.fieldRequired');
    for (const code of ['invalid_value', 'invalid_format', 'invalid_range', 'something_new']) {
      expect(fieldErrorKey(code)).toBe('errors.fieldInvalid');
    }
    for (const key of Object.values(FIELD_CODE_KEYS)) {
      expect(typeof lookup(key)).toBe('string');
    }
  });
});
