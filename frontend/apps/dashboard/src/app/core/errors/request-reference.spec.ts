import { newRequestId, shortReference } from './request-reference';

// The backend keeps a client ID only in this form (observability::request_id::validate).
const BACKEND_FORMAT = /^req_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

describe('newRequestId', () => {
  it('has the form the backend accepts: req_ and a lower-case UUID, 40 characters', () => {
    const id = newRequestId();
    expect(id).toHaveLength(40);
    expect(id).toMatch(BACKEND_FORMAT);
  });

  it('is different every time', () => {
    expect(new Set(Array.from({ length: 50 }, () => newRequestId())).size).toBe(50);
  });

  it('still produces an accepted ID where crypto.randomUUID is missing (insecure context)', () => {
    const real = globalThis.crypto;
    vi.stubGlobal('crypto', { getRandomValues: real.getRandomValues.bind(real) });
    try {
      const id = newRequestId();
      expect(id).toMatch(BACKEND_FORMAT);
      expect(id[4 + 14]).toBe('4'); // version nibble
      expect('89ab').toContain(id[4 + 19]); // variant bits
    } finally {
      vi.unstubAllGlobals();
    }
  });
});

describe('shortReference', () => {
  it('is the last eight characters of the UUID', () => {
    expect(shortReference('req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f')).toBe('3c4d5e6f');
  });

  it('works on an ID without the prefix', () => {
    expect(shortReference('0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f')).toBe('3c4d5e6f');
  });
});
