/**
 * The request ID the dashboard puts on every API call (`X-Request-ID`), and the short reference a
 * person reads out of it (MOH-64).
 *
 * The backend keeps a client ID only when it is `req_` + a lower-case UUID (40 characters,
 * `observability::request_id::validate`); anything else is replaced by its own. A v4 UUID from the
 * browser fits.
 */
export const REQUEST_ID_HEADER = 'X-Request-ID';

/** A new request ID: `req_` and a lower-case v4 UUID. */
export function newRequestId(): string {
  return `req_${uuidV4()}`;
}

/**
 * The part of an ID a person can read out: the last eight characters of its UUID, which are random
 * in IDs from the browser and from the server alike (the front of a server ID is a timestamp).
 */
export function shortReference(requestId: string): string {
  return requestId.replace(/^req_/, '').replaceAll('-', '').slice(-8);
}

function uuidV4(): string {
  const crypto = globalThis.crypto;
  if (typeof crypto?.randomUUID === 'function') return crypto.randomUUID();
  // `randomUUID` exists only in secure contexts; `getRandomValues` does not have that limit.
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
