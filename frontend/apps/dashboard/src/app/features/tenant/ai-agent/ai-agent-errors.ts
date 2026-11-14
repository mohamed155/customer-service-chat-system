import { ApiErrorDetail } from '../../../core/api/api.models';
import { fieldErrorKey } from '../../../core/errors/error-message';
import { marker } from '../../../core/i18n/marker';
import { ownValue } from '../../../core/i18n/own-value';

/** Catalog keys of the field error codes the AI module sends beyond the shared ones. */
const FIELD_CODE_KEYS: Readonly<Record<string, string>> = {
  invalid_length: marker('ai-agent.fieldErrors.length'),
  too_long: marker('ai-agent.fieldErrors.tooLong'),
  too_many: marker('ai-agent.fieldErrors.tooMany'),
  duplicate: marker('ai-agent.fieldErrors.duplicate'),
  must_be_empty: marker('ai-agent.fieldErrors.mustBeEmpty'),
  malformed_placeholder: marker('ai-agent.fieldErrors.malformedPlaceholder'),
  unknown_variable: marker('ai-agent.fieldErrors.unknownVariable'),
};

/** The catalog key of what a field error code says; a code nobody knows says "not valid". */
export function agentFieldErrorKey(code: string): string {
  return ownValue(FIELD_CODE_KEYS, code) ?? fieldErrorKey(code);
}

/**
 * Field errors of a `422`, as catalog keys by the form field they belong to. A nested field
 * (`businessRules[2]`, `providerSelection.model`) is shown with its form field
 * (`businessRules`, `providerSelection`); a field is not told the same thing twice.
 */
export function agentFieldErrors(
  details: readonly ApiErrorDetail[] | undefined,
): Record<string, string[]> {
  const byField: Record<string, string[]> = {};
  for (const detail of details ?? []) {
    if (!detail.field) continue;
    const root = detail.field.split(/[.[]/)[0];
    const keys = (byField[root] ??= []);
    const key = agentFieldErrorKey(detail.code);
    if (!keys.includes(key)) keys.push(key);
  }
  return byField;
}
