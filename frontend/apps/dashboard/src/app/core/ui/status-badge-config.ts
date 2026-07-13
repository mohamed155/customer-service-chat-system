import { BadgeTone } from '../../shared/components/status-badge/status-badge.component';
import { InvitationStatus, MemberStatus, MembershipRole } from '../api/tenant-api.models';

export const MEMBER_STATUS_TONES: Record<MemberStatus, BadgeTone> = {
  active: 'green',
  disabled: 'neutral',
};

export const INVITATION_STATUS_TONES: Record<InvitationStatus, BadgeTone> = {
  pending: 'amber',
  accepted: 'green',
  revoked: 'neutral',
  expired: 'neutral',
};

export const MEMBERSHIP_ROLE_TONES: Record<MembershipRole, BadgeTone> = {
  owner: 'accent',
  admin: 'accent',
  manager: 'neutral',
  agent: 'neutral',
  viewer: 'neutral',
};
