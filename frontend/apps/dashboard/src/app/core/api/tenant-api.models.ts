export type TenantStatus = 'active' | 'suspended';

export type MembershipRole = 'owner' | 'admin' | 'manager' | 'agent' | 'viewer';

export type PlatformRole = 'super_admin' | 'developer' | 'sales' | 'support' | 'finance';

export interface TenantSummary {
  readonly id: string;
  readonly name: string;
  readonly slug: string;
  readonly status: TenantStatus;
}

export interface MembershipSummary {
  readonly tenantId: string;
  readonly tenantName: string;
  readonly tenantSlug: string;
  readonly role: MembershipRole;
}

export interface MeResponse {
  readonly id: string;
  readonly email: string;
  readonly displayName: string;
  readonly platformRole: PlatformRole | null;
  readonly memberships: MembershipSummary[];
}

export interface TenantDirectoryParams {
  readonly cursor?: string;
  readonly limit?: number;
  readonly q?: string;
}