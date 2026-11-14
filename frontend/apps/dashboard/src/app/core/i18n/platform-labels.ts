import { marker } from './marker';
import { ownValue } from './own-value';

/** Catalog keys of the names of a tenant's status and plan. A value this app has no name for is
 * shown as the API sent it. */
const TENANT_STATUS_KEYS: Readonly<Record<string, string>> = {
  active: marker('platform.tenants.active'),
  suspended: marker('platform.tenants.suspended'),
};

const PLAN_KEYS: Readonly<Record<string, string>> = {
  trial: marker('platform.tenants.plans.trial'),
  starter: marker('platform.tenants.plans.starter'),
  professional: marker('platform.tenants.plans.professional'),
  enterprise: marker('platform.tenants.plans.enterprise'),
};

const PLAN_STATUS_KEYS: Readonly<Record<string, string>> = {
  active: marker('platform.billing.planStatus.active'),
  retired: marker('platform.billing.planStatus.retired'),
};

const SUBSCRIPTION_KEYS: Readonly<Record<string, string>> = {
  active: marker('platform.billing.subscriptionStatus.active'),
  trialing: marker('platform.billing.subscriptionStatus.trialing'),
  past_due: marker('platform.billing.subscriptionStatus.pastDue'),
  canceled: marker('platform.billing.subscriptionStatus.canceled'),
  incomplete: marker('platform.billing.subscriptionStatus.incomplete'),
  incomplete_expired: marker('platform.billing.subscriptionStatus.incompleteExpired'),
};

/** Why a call to an AI provider failed (`errorCategory` in the provider status). */
const FAILURE_CATEGORY_KEYS: Readonly<Record<string, string>> = {
  authentication: marker('platform.providers.failureCategory.authentication'),
  rate_limited: marker('platform.providers.failureCategory.rateLimited'),
  unavailable: marker('platform.providers.failureCategory.unavailable'),
  timeout: marker('platform.providers.failureCategory.timeout'),
  invalid_request: marker('platform.providers.failureCategory.invalidRequest'),
};

const PROVIDER_VERDICT_KEYS: Readonly<Record<string, string>> = {
  no_activity: marker('platform.providers.verdict.noActivity'),
  insufficient_data: marker('platform.providers.verdict.insufficientData'),
};

export const tenantStatusKey = (v: string): string | null =>
  ownValue(TENANT_STATUS_KEYS, v) ?? null;
export const planKey = (v: string): string | null => ownValue(PLAN_KEYS, v) ?? null;
export const planStatusKey = (v: string): string | null => ownValue(PLAN_STATUS_KEYS, v) ?? null;
export const subscriptionKey = (v: string): string | null => ownValue(SUBSCRIPTION_KEYS, v) ?? null;
export const failureCategoryKey = (v: string): string | null =>
  ownValue(FAILURE_CATEGORY_KEYS, v) ?? null;
export const providerVerdictKey = (v: string): string | null =>
  ownValue(PROVIDER_VERDICT_KEYS, v) ?? null;
