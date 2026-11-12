import {
  apiRelativePath,
  isIdentityEndpoint,
  isLoginEndpoint,
  isPlatformEndpoint,
} from './api-request';

describe('apiRelativePath', () => {
  it('returns the path below a relative API base', () => {
    expect(apiRelativePath('/api/v1/me', '/api/v1')).toBe('/me');
    expect(apiRelativePath('/api/v1/tenant/members?limit=5#x', '/api/v1/')).toBe('/tenant/members');
  });

  it('returns the path below an absolute API base on the same origin', () => {
    const base = 'http://localhost:8080/api/v1';
    expect(apiRelativePath(`${base}/me`, base)).toBe('/me');
    expect(apiRelativePath(`${base}/platform/tenants`, base)).toBe('/platform/tenants');
  });

  it('returns null for anything outside the API', () => {
    expect(apiRelativePath('/assets/logo.svg', '/api/v1')).toBeNull();
    expect(apiRelativePath('/api/v10/me', '/api/v1')).toBeNull();
    expect(apiRelativePath('https://other.test/api/v1/me', 'https://app.test/api/v1')).toBeNull();
  });
});

describe('endpoint helpers', () => {
  it('matches the identity endpoint exactly', () => {
    expect(isIdentityEndpoint('/me')).toBe(true);
    expect(isIdentityEndpoint('/tenant/members')).toBe(false);
    expect(isIdentityEndpoint('/tenant/availability/me')).toBe(false);
    expect(isIdentityEndpoint('/tenant/conversations/c1/messages')).toBe(false);
    expect(isIdentityEndpoint('/me/other')).toBe(false);
    expect(isIdentityEndpoint(null)).toBe(false);
  });

  it('matches platform routes by path segment', () => {
    expect(isPlatformEndpoint('/platform/tenants')).toBe(true);
    expect(isPlatformEndpoint('/platform')).toBe(true);
    expect(isPlatformEndpoint('/tenant/platform-notes')).toBe(false);
    expect(isPlatformEndpoint('/tenant/x/platform/y')).toBe(false);
    expect(isPlatformEndpoint(null)).toBe(false);
  });

  it('matches the login endpoint exactly', () => {
    expect(isLoginEndpoint('/auth/login')).toBe(true);
    expect(isLoginEndpoint('/auth/login-history')).toBe(false);
    expect(isLoginEndpoint(null)).toBe(false);
  });
});
