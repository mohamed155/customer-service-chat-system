/**
 * `map[key]` for a `key` that came from outside (an API `code`, a role, a routing reason): only the
 * map's own entries count. A plain object also answers for `constructor`, `toString` or
 * `__proto__`, and a function in place of a catalog key would break the screen that shows it.
 */
export function ownValue<T>(
  map: Readonly<Record<string | number, T>>,
  key: string | number,
): T | undefined {
  return Object.hasOwn(map, key) ? map[key] : undefined;
}
