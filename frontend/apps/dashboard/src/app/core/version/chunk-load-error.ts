/**
 * Whether an error is the browser failing to fetch a lazy-loaded script or stylesheet (MOH-167).
 *
 * After a deploy the files of the old release are gone, so a lazy route of a page that was opened
 * before the deploy fails to load. The browsers word it differently: Chrome and Edge ("Failed to
 * fetch dynamically imported module"), Firefox ("error loading dynamically imported module"),
 * Safari ("Importing a module script failed"), and webpack-style loaders ("Loading chunk 12
 * failed", `ChunkLoadError`).
 */
const CHUNK_LOAD_MESSAGE =
  /failed to fetch dynamically imported module|error loading dynamically imported module|importing a module script failed|loading (?:css )?chunk [\w-]+ failed|unable to preload css/i;

export function isChunkLoadError(error: unknown): boolean {
  if (!(error instanceof Error)) return false;
  return error.name === 'ChunkLoadError' || CHUNK_LOAD_MESSAGE.test(error.message);
}
