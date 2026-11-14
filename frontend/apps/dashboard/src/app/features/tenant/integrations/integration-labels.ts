import { marker } from '../../../core/i18n/marker';
import { ownValue } from '../../../core/i18n/own-value';

/** Catalog keys of an integration's connection status. */
const STATUS_KEYS: Readonly<Record<string, string>> = {
  connected: marker('integrations.status.connected'),
  disconnected: marker('integrations.status.disconnected'),
  not_connected: marker('integrations.status.notConnected'),
  error: marker('integrations.status.error'),
};

/** Catalog keys of the event types the API logs. */
const EVENT_KEYS: Readonly<Record<string, string>> = {
  connected: marker('integrations.event.connected'),
  config_updated: marker('integrations.event.configUpdated'),
  secret_rotated: marker('integrations.event.secretRotated'),
  disconnected: marker('integrations.event.disconnected'),
  delivery_accepted: marker('integrations.event.deliveryAccepted'),
  delivery_rejected: marker('integrations.event.deliveryRejected'),
};

/** Catalog keys of the outcome of an event. */
const OUTCOME_KEYS: Readonly<Record<string, string>> = {
  success: marker('integrations.outcome.success'),
  failure: marker('integrations.outcome.failure'),
};

/** Catalog keys of why a delivery was rejected (`RejectionReason` in the API). */
const REASON_KEYS: Readonly<Record<string, string>> = {
  invalid_signature: marker('integrations.reason.invalidSignature'),
  inactive_connection: marker('integrations.reason.inactiveConnection'),
  payload_too_large: marker('integrations.reason.payloadTooLarge'),
  rate_limited: marker('integrations.reason.rateLimited'),
  malformed_payload: marker('integrations.reason.malformedPayload'),
};

export const statusKey = (status: string): string | null => ownValue(STATUS_KEYS, status) ?? null;
export const eventKey = (type: string): string | null => ownValue(EVENT_KEYS, type) ?? null;
export const outcomeKey = (outcome: string): string | null =>
  ownValue(OUTCOME_KEYS, outcome) ?? null;
export const reasonKey = (reason: string): string | null => ownValue(REASON_KEYS, reason) ?? null;
