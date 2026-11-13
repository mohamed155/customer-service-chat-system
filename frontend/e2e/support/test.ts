import { expect, test as base } from '@playwright/test';

/**
 * `test` for every spec that runs against the dashboard build. The static server sends that build
 * with the production Content-Security-Policy, Trusted Types included (e2e/support/csp.cjs), so
 * any CSP or Trusted Types violation the page reports during a test fails it (MOH-38, MOH-170).
 * A violation is a blocked script, style, image, connection or an unsafe string written to an HTML
 * sink: a feature that is silently broken in production.
 *
 * Specs that provoke a violation on purpose (csp-enforcement.spec.ts) import from
 * '@playwright/test' instead and record violations themselves.
 */
export const test = base.extend<{ policyGuard: void }>({
  policyGuard: [
    async ({ page }, use) => {
      const violations: string[] = [];
      // Survives navigations: the binding is added again to every page the test loads.
      await page.exposeFunction('__reportPolicyViolation', (message: string) => {
        violations.push(message);
      });
      await page.addInitScript(() => {
        document.addEventListener('securitypolicyviolation', (event) => {
          const report = (window as unknown as { __reportPolicyViolation?: (m: string) => void })
            .__reportPolicyViolation;
          report?.(
            `${event.violatedDirective} blocked ${event.blockedURI || 'inline'}` +
              (event.sample ? ` (${event.sample.slice(0, 120)})` : ''),
          );
        });
      });
      await use();
      expect(violations, 'Content-Security-Policy or Trusted Types violations').toEqual([]);
    },
    { auto: true },
  ],
});

export { expect };
