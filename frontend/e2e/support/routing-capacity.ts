import type { components } from '../../apps/dashboard/src/app/core/api/generated/api';

/** Capacity endpoint response for mocked team pages. */
export function routingCapacity(
  members: readonly { id: string; displayName: string }[] = [],
): components['schemas']['RoutingCapacityResponse'] {
  return {
    default_capacity: 10,
    agents: members.map((member) => ({
      membership_id: member.id,
      display_name: member.displayName,
      override: null,
      effective: 10,
      open_load: 0,
    })),
  };
}
