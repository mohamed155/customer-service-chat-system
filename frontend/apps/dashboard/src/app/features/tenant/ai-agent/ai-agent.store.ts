import { computed, inject } from '@angular/core';
import {
  patchState,
  signalStore,
  withComputed,
  withHooks,
  withMethods,
  withState,
} from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { catchError, EMPTY, forkJoin, pipe, switchMap, tap } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import {
  AgentConfigPayload,
  AgentConfigResponse,
  AgentOptionsResponse,
} from '../../../core/api/ai-agent.models';
import { AiAgentApiService } from './ai-agent-api.service';

interface AiAgentState {
  config: AgentConfigResponse | null;
  options: AgentOptionsResponse | null;
  loading: boolean;
  saving: boolean;
  error: string | null;
  conflict: boolean;
  fieldErrors: Record<string, string[]> | null;
  activeTab: 'behavior' | 'prompt' | 'escalation';
}

const initialState: AiAgentState = {
  config: null,
  options: null,
  loading: false,
  saving: false,
  error: null,
  conflict: false,
  fieldErrors: null,
  activeTab: 'behavior',
};

export type AiAgentTab = AiAgentState['activeTab'];

export const AiAgentStore = signalStore(
  withState(initialState),
  withComputed((store) => ({
    isConfigured: computed(() => store.config()?.configured ?? false),
    hasConflict: computed(() => store.conflict()),
    fieldErrors: computed(() => store.fieldErrors()),
    brokenSkillRefs: computed(() => {
      const rules = store.config()?.agent.escalationRules ?? [];
      return rules.flatMap((r) => r.brokenSkillRefs ?? []);
    }),
    staleProviderSelection: computed(() => store.config()?.agent.providerSelection.stale ?? false),
  })),
  withMethods((store, api = inject(AiAgentApiService)) => ({
    load: rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null })),
        switchMap(() =>
          forkJoin({
            config: api.getAgent(),
            options: api.getOptions(),
          }).pipe(
            tap(({ config, options }) =>
              patchState(store, {
                config: config.data,
                options: options.data,
                loading: false,
              }),
            ),
            catchError((err) => {
              patchState(store, { loading: false, error: (err as ApiError).message });
              return EMPTY;
            }),
          ),
        ),
      ),
    ),
    save(data: AgentConfigPayload): void {
      patchState(store, { saving: true, error: null, conflict: false, fieldErrors: null });
      api.saveAgent(data).subscribe({
        next: (res) => patchState(store, { config: res.data, saving: false }),
        error: (err: ApiError) => {
          if (err.status === 409) {
            patchState(store, { saving: false, conflict: true, error: err.message });
          } else if (err.status === 422 && err.details) {
            const fieldErrors: Record<string, string[]> = {};
            for (const d of err.details) {
              if (d.field) {
                (fieldErrors[d.field] ??= []).push(d.message);
              }
            }
            patchState(store, { saving: false, fieldErrors });
          } else {
            patchState(store, { saving: false, error: err.message });
          }
        },
      });
    },
    setTab(tab: AiAgentTab): void {
      patchState(store, { activeTab: tab });
    },
    dismissConflict(): void {
      patchState(store, { conflict: false });
    },
  })),
  withHooks({
    onInit(store) {
      store.load();
    },
  }),
);
