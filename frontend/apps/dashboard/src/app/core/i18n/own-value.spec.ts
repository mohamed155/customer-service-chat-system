import { ownValue } from './own-value';

describe('ownValue', () => {
  const map = { a: 'x', 1: 'one' };

  it('returns the map’s own entries', () => {
    expect(ownValue(map, 'a')).toBe('x');
    expect(ownValue(map, 1)).toBe('one');
  });

  it('returns nothing for a missing key or a property every object has', () => {
    for (const key of ['b', 'constructor', 'toString', '__proto__', 'hasOwnProperty']) {
      expect(ownValue(map, key), key).toBeUndefined();
    }
  });
});
