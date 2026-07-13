import { effect, inject } from '@angular/core';
import { Store } from '@ngrx/store';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import {
  catchError,
  debounceTime,
  distinctUntilChanged,
  EMPTY,
  exhaustMap,
  mergeMap,
  pipe,
  retry,
  switchMap,
  takeWhile,
  tap,
  throwError,
  timer,
} from 'rxjs';
import {
  CreateInvitationPayload,
  CreateInvitationResponse,
  InvitationQuery,
  InvitationStatus,
  MemberStatus,
  MembershipRole,
  PatchMemberPayload,
  TeamMember,
  TeamMemberQuery,
  TenantInvitation,
} from '../../../core/api/tenant-api.models';
import { selectActiveTenant } from '../../../core/state/tenant-context.feature';
import { TeamApiService } from './team-api.service';

export type TeamListStatus = 'pending' | 'loading' | 'success' | 'error' | 'empty';
export type MemberMutationStatus = 'idle' | 'submitting' | 'success' | 'error';

interface TeamState {
  members: TeamMember[];
  status: TeamListStatus;
  query: TeamMemberQuery;
  nextCursor: string | null;
  hasMore: boolean;
  error: string | null;
  invitations: TenantInvitation[];
  invitationQuery: InvitationQuery;
  invitationNextCursor: string | null;
  invitationHasMore: boolean;
  invitationsStatus: 'pending' | 'loading' | 'success' | 'error';
  invitationsError: string | null;
  lastCreatedInvitation: CreateInvitationResponse | null;
  createInvitationStatus: 'idle' | 'submitting' | 'success' | 'error';
  createInvitationError: string | null;
  invitationDeliveryPollingError: string | null;
  memberUpdateStatus: MemberMutationStatus;
  memberUpdateError: string | null;
  memberUpdateMemberId: string | null;
  revokeInvitationStatus: MemberMutationStatus;
  revokeInvitationError: string | null;
  revokeInvitationId: string | null;
}

const initialState: TeamState = {
  members: [],
  status: 'pending',
  query: { limit: 25 },
  nextCursor: null,
  hasMore: false,
  error: null,
  invitations: [],
  invitationQuery: { limit: 25 },
  invitationNextCursor: null,
  invitationHasMore: false,
  invitationsStatus: 'pending',
  invitationsError: null,
  lastCreatedInvitation: null,
  createInvitationStatus: 'idle',
  createInvitationError: null,
  invitationDeliveryPollingError: null,
  memberUpdateStatus: 'idle',
  memberUpdateError: null,
  memberUpdateMemberId: null,
  revokeInvitationStatus: 'idle',
  revokeInvitationError: null,
  revokeInvitationId: null,
};

export const TeamStore = signalStore(
  withState(initialState),
  withMethods((store, api = inject(TeamApiService)) => {
    const loadMembers = rxMethod<{ query: TeamMemberQuery; append?: boolean }>(
      pipe(
        tap(() => patchState(store, { status: 'loading' as const, error: null })),
        switchMap(({ query, append }) =>
          api.getMembers(query).pipe(
            tap({
              next: (response) => {
                const data = response.data;
                patchState(store, {
                  members: append ? [...store.members(), ...data.items] : data.items,
                  nextCursor: data.nextCursor ?? null,
                  hasMore: data.hasMore ?? false,
                  status:
                    append || data.items.length > 0 ? ('success' as const) : ('empty' as const),
                });
              },
              error: (err) =>
                patchState(store, {
                  status: 'error' as const,
                  error: err?.message ?? 'Failed to load members',
                }),
            }),
            catchError(() => EMPTY),
          ),
        ),
      ),
    );

    const loadInvitations = rxMethod<{ query: InvitationQuery; append?: boolean }>(
      pipe(
        tap(() =>
          patchState(store, {
            invitationsStatus: 'loading' as const,
            invitationsError: null,
          }),
        ),
        switchMap(({ query, append }) =>
          api.getInvitations(query).pipe(
            tap({
              next: (response) =>
                patchState(store, {
                  invitations: append
                    ? [...store.invitations(), ...response.data.items]
                    : response.data.items,
                  invitationNextCursor: response.data.nextCursor ?? null,
                  invitationHasMore: response.data.hasMore ?? false,
                  invitationsStatus: 'success' as const,
                }),
              error: (err) =>
                patchState(store, {
                  invitationsStatus: 'error' as const,
                  invitationsError: err?.message ?? 'Failed to load invitations',
                }),
            }),
            catchError(() => EMPTY),
          ),
        ),
      ),
    );

    const invitationBaseQuery = (): InvitationQuery => {
      const query = { ...store.invitationQuery() };
      delete query.cursor;
      return query;
    };

    const pollInvitationDelivery = rxMethod<string | null>(
      pipe(
        switchMap((invitationId) => {
          if (!invitationId) return EMPTY;
          return timer(0, 1000).pipe(
            exhaustMap(() =>
              api.getInvitationDelivery(invitationId).pipe(
                retry({
                  count: 2,
                  delay: (error) => {
                    const status = (error as { status?: number })?.status;
                    return status != null && status >= 400 && status < 500
                      ? throwError(() => error)
                      : timer(1000);
                  },
                }),
              ),
            ),
            tap((response) => {
              const deliveryStatus = response.data.emailDeliveryStatus;
              const invitations = store
                .invitations()
                .map((item) =>
                  item.id === invitationId
                    ? { ...item, emailDeliveryStatus: deliveryStatus }
                    : item,
                );
              const invitation = invitations.find((item) => item.id === invitationId);
              patchState(store, {
                invitations,
                invitationDeliveryPollingError: null,
              });
              const created = store.lastCreatedInvitation();
              if (invitation && created?.invitation.id === invitationId) {
                patchState(store, {
                  lastCreatedInvitation: {
                    ...created,
                    invitation,
                    emailDeliveryStatus: deliveryStatus,
                    emailSent: deliveryStatus === 'sent',
                  },
                });
              }
            }),
            takeWhile((response) => response.data.emailDeliveryStatus === 'queued', true),
            catchError(() => {
              patchState(store, {
                invitationDeliveryPollingError:
                  'Unable to refresh invitation email delivery status. Try again later.',
              });
              return EMPTY;
            }),
          );
        }),
      ),
    );

    const resetInvitationPagination = (query: InvitationQuery): void => {
      patchState(store, {
        invitations: [],
        invitationQuery: query,
        invitationNextCursor: null,
        invitationHasMore: false,
      });
    };

    const createInvitation = rxMethod<CreateInvitationPayload>(
      pipe(
        tap(() =>
          patchState(store, {
            createInvitationStatus: 'submitting' as const,
            createInvitationError: null,
          }),
        ),
        switchMap((payload) =>
          api.createInvitation(payload).pipe(
            tap({
              next: (response) => {
                patchState(store, {
                  lastCreatedInvitation: response.data,
                  invitations: [
                    response.data.invitation,
                    ...store
                      .invitations()
                      .filter((item) => item.id !== response.data.invitation.id),
                  ],
                  invitationDeliveryPollingError: null,
                });
                patchState(store, { createInvitationStatus: 'success' as const });
                if (response.data.emailDeliveryStatus === 'queued') {
                  pollInvitationDelivery(response.data.invitation.id);
                } else {
                  loadInvitations({ query: invitationBaseQuery() });
                }
                loadMembers({ query: store.query() });
              },
              error: (err) =>
                patchState(store, {
                  createInvitationStatus: 'error' as const,
                  createInvitationError: err?.message ?? 'Failed to create invitation',
                }),
            }),
            catchError(() => EMPTY),
          ),
        ),
      ),
    );

    const revokeInvitation = rxMethod<string>(
      pipe(
        tap((id) =>
          patchState(store, {
            revokeInvitationStatus: 'submitting' as const,
            revokeInvitationError: null,
            revokeInvitationId: id,
          }),
        ),
        mergeMap((id) =>
          api.revokeInvitation(id).pipe(
            tap({
              next: () => {
                patchState(store, {
                  revokeInvitationStatus: 'success' as const,
                  revokeInvitationError: null,
                  revokeInvitationId: null,
                });
                loadInvitations({ query: invitationBaseQuery() });
              },
              error: (err) =>
                patchState(store, {
                  revokeInvitationStatus: 'error' as const,
                  revokeInvitationError: err?.message ?? 'Failed to revoke invitation',
                  revokeInvitationId: id,
                }),
            }),
            catchError(() => EMPTY),
          ),
        ),
      ),
    );

    const updateMember = rxMethod<{ id: string; payload: PatchMemberPayload }>(
      pipe(
        tap(({ id }) =>
          patchState(store, {
            memberUpdateStatus: 'submitting' as const,
            memberUpdateError: null,
            memberUpdateMemberId: id,
          }),
        ),
        mergeMap(({ id, payload }) =>
          api.patchMember(id, payload).pipe(
            tap({
              next: () => {
                patchState(store, {
                  memberUpdateStatus: 'success' as const,
                  memberUpdateError: null,
                  memberUpdateMemberId: null,
                });
                loadMembers({ query: store.query() });
              },
              error: (err) =>
                patchState(store, {
                  memberUpdateStatus: 'error' as const,
                  memberUpdateError: err?.message ?? 'Failed to update member',
                  memberUpdateMemberId: id,
                }),
            }),
            catchError(() => EMPTY),
          ),
        ),
      ),
    );

    const searchMembers = rxMethod<string>(
      pipe(
        debounceTime(300),
        distinctUntilChanged(),
        tap((q) => {
          const updatedQuery = { ...store.query(), q: q || undefined, cursor: undefined };
          patchState(store, { query: updatedQuery });
          loadMembers({ query: updatedQuery });
        }),
      ),
    );

    return {
      loadMembers,
      loadInvitations,
      pollInvitationDelivery,
      createInvitation,
      revokeInvitation,
      updateMember,
      searchMembers,
      setQuery(query: Partial<TeamMemberQuery>): void {
        patchState(store, { query: { ...store.query(), ...query } });
      },
      setInvitationQuery(query: Partial<InvitationQuery>): void {
        patchState(store, { invitationQuery: { ...store.invitationQuery(), ...query } });
      },
      setInvitationStatusFilter(status: InvitationStatus | undefined): void {
        const updatedQuery = { ...store.invitationQuery(), status, cursor: undefined };
        resetInvitationPagination(updatedQuery);
        loadInvitations({ query: updatedQuery });
      },
      search(q: string): void {
        searchMembers(q);
      },
      changeMemberRole(id: string, role: MembershipRole): void {
        updateMember({ id, payload: { role } });
      },
      setMemberStatus(id: string, status: MemberStatus): void {
        updateMember({ id, payload: { status } });
      },
      clearMemberUpdateError(): void {
        patchState(store, {
          memberUpdateStatus: 'idle',
          memberUpdateError: null,
          memberUpdateMemberId: null,
        });
      },
      clearRevokeInvitationError(): void {
        patchState(store, {
          revokeInvitationStatus: 'idle',
          revokeInvitationError: null,
          revokeInvitationId: null,
        });
      },
      filterByStatus(status: MemberStatus | undefined): void {
        const updatedQuery = { ...store.query(), status, cursor: undefined };
        patchState(store, { query: updatedQuery });
        loadMembers({ query: updatedQuery });
      },
      loadNext(): void {
        if (!store.hasMore() || !store.nextCursor()) return;
        const query = { ...store.query(), cursor: store.nextCursor()! };
        patchState(store, { query });
        loadMembers({ query, append: true });
      },
      retry(): void {
        loadMembers({ query: store.query() });
      },
      clearLastCreatedInvitation(): void {
        pollInvitationDelivery(null);
        patchState(store, {
          lastCreatedInvitation: null,
          invitationDeliveryPollingError: null,
        });
      },
      loadMoreInvitations(): void {
        if (!store.invitationHasMore() || !store.invitationNextCursor()) return;
        const query = { ...store.invitationQuery(), cursor: store.invitationNextCursor()! };
        patchState(store, { invitationQuery: query });
        loadInvitations({ query, append: true });
      },
    };
  }),
  withHooks((store) => {
    const globalStore = inject(Store, { optional: true });
    const activeTenant = globalStore?.selectSignal(selectActiveTenant) ?? (() => null);
    let initialized = false;
    let previousTenantId: string | null = null;

    effect(() => {
      const tenantId = activeTenant()?.id ?? null;
      if (!initialized) {
        initialized = true;
        previousTenantId = tenantId;
        return;
      }

      if (tenantId === previousTenantId) return;
      previousTenantId = tenantId;
      store.pollInvitationDelivery(null);

      patchState(store, {
        members: [],
        status: 'pending' as const,
        query: { limit: 25 },
        nextCursor: null,
        hasMore: false,
        error: null,
        invitations: [],
        invitationQuery: { limit: 25 },
        invitationNextCursor: null,
        invitationHasMore: false,
        invitationsStatus: 'pending' as const,
        invitationsError: null,
        lastCreatedInvitation: null,
        createInvitationStatus: 'idle' as const,
        createInvitationError: null,
        invitationDeliveryPollingError: null,
      });

      store.loadMembers({ query: store.query() });
      store.loadInvitations({ query: store.invitationQuery() });
    });

    return {
      onInit(): void {
        store.loadMembers({ query: store.query() });
        store.loadInvitations({ query: store.invitationQuery() });
      },
    };
  }),
);
