import { roleNameKey } from '../../../core/i18n/labels';
import { marker } from '../../../core/i18n/marker';
import { ownValue } from '../../../core/i18n/own-value';

/** Catalog keys (shared catalog) of member and invitation statuses. */
const STATUS_KEYS: Readonly<Record<string, string>> = {
  active: marker('status.active'),
  disabled: marker('status.disabled'),
  pending: marker('status.pending'),
  expired: marker('status.expired'),
  accepted: marker('status.accepted'),
  revoked: marker('status.revoked'),
};

/** The key of a role's name; an unknown role gets the generic name of a member. */
export function roleLabelKey(role: string): string {
  return roleNameKey(role) ?? marker('team.members.unknownRole');
}

export function invitationStatusKey(status: string): string {
  return ownValue(STATUS_KEYS, status) ?? marker('team.members.unknownStatus');
}
