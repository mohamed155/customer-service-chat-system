import { patchState, signalStore, withMethods, withState } from '@ngrx/signals';

export type AiAgentTab = 'behavior' | 'prompt' | 'escalation' | 'testing';

export const AiAgentStore = signalStore(
  withState({ activeTab: 'behavior' as AiAgentTab }),
  withMethods((store) => ({
    setTab(activeTab: AiAgentTab): void {
      patchState(store, { activeTab });
    },
  })),
);
