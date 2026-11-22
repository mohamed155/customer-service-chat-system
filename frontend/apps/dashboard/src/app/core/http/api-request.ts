/**
 * Which API endpoint a request targets. Interceptors decide by exact path segments below the API
 * base URL, never by a substring: `tenant/members` and `tenant/availability/me` both contain `/me`
 * and are not the identity endpoint (MOH-60).
 */

/** The request path below the API base URL (`/me`, `/tenant/members`), or `null` for any other URL. */
export function apiRelativePath(requestUrl: string, apiBaseUrl: string): string | null {
  if (!targetsApiBaseUrl(requestUrl, apiBaseUrl)) {
    return null;
  }
  const basePath = pathnameFromUrl(normalizeBase(apiBaseUrl));
  const rest = pathnameFromUrl(requestUrl).slice(basePath === '/' ? 0 : basePath.length);
  return rest === '' ? '/' : rest;
}

/** `GET /me`: the signed-in user, requested before a workspace is chosen. */
export function isIdentityEndpoint(relativePath: string | null): boolean {
  return relativePath === '/me';
}

/** Platform (operator) routes are not scoped to a workspace. */
export function isPlatformEndpoint(relativePath: string | null): boolean {
  return relativePath === '/platform' || (relativePath?.startsWith('/platform/') ?? false);
}

/** Workspace data: everything under `/tenant/`. */
export function isTenantEndpoint(relativePath: string | null): boolean {
  return relativePath?.startsWith('/tenant/') ?? false;
}

/** The sign-in call: a 401 here is a wrong password, not an expired session. */
export function isLoginEndpoint(relativePath: string | null): boolean {
  return relativePath === '/auth/login';
}

/** The refresh call: `POST /auth/refresh` exchanges the refresh cookie for a new access token. */
export function isRefreshEndpoint(relativePath: string | null): boolean {
  return relativePath === '/auth/refresh';
}

/**
 * Calls that start a session, or the refresh itself. A `401` from one of them is an answer about
 * that call (a wrong password, a lapsed second-step challenge, a refresh token that is no longer
 * good), not an expired access token, so none of them is refreshed and retried. Logout is not in
 * the list: the access cookie lives ten minutes, so a person who signs out after that gets a `401`
 * and has to be refreshed first, or the session would stay alive on the server.
 */
export function isSessionEndpoint(relativePath: string | null): boolean {
  return (
    relativePath === '/auth/login' ||
    relativePath === '/auth/signup' ||
    relativePath === '/auth/mfa/verify' ||
    isRefreshEndpoint(relativePath)
  );
}

export function targetsApiBaseUrl(requestUrl: string, apiBaseUrl: string): boolean {
  const apiBase = normalizeBase(apiBaseUrl);
  const apiPath = pathnameFromUrl(apiBase);
  const requestPath = pathnameFromUrl(requestUrl);

  if (!pathMatchesBase(requestPath, apiPath)) {
    return false;
  }

  if (!isAbsoluteUrl(requestUrl)) {
    return true;
  }

  // An absolute request URL must be on the API's origin. A relative base means the page's own
  // origin, so `https://other.test/api/v1/...` is not ours even though its path matches.
  const apiOrigin = isAbsoluteUrl(apiBase) ? new URL(apiBase).origin : pageOrigin();
  return apiOrigin !== null && new URL(requestUrl).origin === apiOrigin;
}

function pageOrigin(): string | null {
  return typeof location === 'undefined' ? null : location.origin;
}

function normalizeBase(url: string): string {
  return url.replace(/\/+$/, '');
}

function pathnameFromUrl(url: string): string {
  if (!isAbsoluteUrl(url)) {
    return `/${url.replace(/^\//, '').split(/[?#]/, 1)[0].replace(/\/+$/, '')}`;
  }

  return new URL(url).pathname.replace(/\/+$/, '');
}

function pathMatchesBase(path: string, basePath: string): boolean {
  return path === basePath || path.startsWith(`${basePath}/`);
}

function isAbsoluteUrl(url: string): boolean {
  return /^https?:\/\//i.test(url);
}
