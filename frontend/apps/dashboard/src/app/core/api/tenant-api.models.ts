import { Permission } from '../authz/permissions';

export type TenantStatus = 'active' | 'suspended';

export type MembershipRole = 'owner' | 'admin' | 'manager' | 'agent' | 'viewer';

export type PlatformRole = 'super_admin' | 'developer' | 'sales' | 'support' | 'finance';

export type TenantPlan = 'trial' | 'starter' | 'professional' | 'enterprise';

export interface TenantSummary {
  readonly id: string;
  readonly name: string;
  readonly slug: string;
  readonly status: TenantStatus;
  readonly plan: TenantPlan;
}

export interface MembershipSummary {
  readonly tenantId: string;
  readonly tenantName: string;
  readonly tenantSlug: string;
  readonly role: MembershipRole;
  readonly permissions: Permission[];
}

export interface MeResponse {
  readonly id: string;
  readonly email: string;
  readonly displayName: string;
  readonly platformRole: PlatformRole | null;
  readonly platformPermissions: Permission[];
  readonly staffTenantPermissions: Permission[] | null;
  readonly memberships: MembershipSummary[];
}

export interface TenantDirectoryParams {
  readonly cursor?: string;
  readonly limit?: number;
  readonly q?: string;
}

export interface PlatformTenantDetail {
  readonly id: string;
  readonly name: string;
  readonly slug: string;
  readonly status: TenantStatus;
  readonly plan: TenantPlan;
  readonly contactName: string | null;
  readonly contactEmail: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface CreateTenantPayload {
  readonly name: string;
  readonly slug: string;
  readonly plan?: TenantPlan;
  readonly contactName?: string;
  readonly contactEmail?: string;
}

export interface UpdateTenantPayload {
  readonly name?: string;
  readonly slug?: string;
  readonly plan?: TenantPlan;
  readonly status?: TenantStatus;
  readonly contactName?: string | null;
  readonly contactEmail?: string | null;
}

export interface TenantDirectoryQuery {
  readonly q?: string;
  readonly status?: TenantStatus;
  readonly cursor?: string;
  readonly limit?: number;
}
