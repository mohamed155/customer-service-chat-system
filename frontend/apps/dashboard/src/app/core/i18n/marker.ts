/**
 * Marks a string as a catalog key where the code (not a template) holds it, so a reader and
 * `pnpm lint:i18n` can see it is one: `label: marker('platformNav.tenants')`. It returns its
 * argument; the text is looked up where it is shown (`{{ label | transloco }}`).
 */
export function marker<T extends string>(key: T): T {
  return key;
}
